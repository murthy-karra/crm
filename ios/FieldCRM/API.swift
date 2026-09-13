import Foundation

struct APIError: Error, LocalizedError {
    let status: Int, code: String
    var errorDescription: String? {
        switch code {
        case "revision_conflict": "This task changed on the server. Review its refreshed version before choosing another action."
        case "not_found": "This action cannot be confirmed or the record is no longer available. Saved input is retained."
        case "workspace_in_migration_review": "This workspace is held for migration review. Saved work is protected."
        case "protocol_unsupported": "This app must be updated before synchronizing. Saved work is retained."
        case "invalid_credentials": "Email or password was not accepted."
        case "dependency_pending": "Waiting for the task creation to be accepted."
        default: "Sync needs attention (\(code)). Your saved work is retained."
        }
    }
}
private final class NoRedirect: NSObject, URLSessionTaskDelegate, @unchecked Sendable {
    func urlSession(_ session: URLSession, task: URLSessionTask, willPerformHTTPRedirection response: HTTPURLResponse,
                    newRequest request: URLRequest, completionHandler: @escaping (URLRequest?) -> Void) { completionHandler(nil) }
}
@MainActor final class API {
    let base: URL
    var cookie = ""
    private let session: URLSession
    #if DEBUG
    // Test seam drops an actual successful HTTP response after the server committed it.
    var dropNextOperationResponse = false
    var responseForTesting: ((URLRequest) async throws -> (Data, HTTPURLResponse))?
    #endif
    init(base: String) throws {
        guard let url = URL(string: base), url.user == nil, url.password == nil, url.query == nil,
              url.fragment == nil, url.path.isEmpty || url.path == "/" else { throw LocalError.invalidProtocol }
        var permitted = url.scheme == "https"
        #if DEBUG && targetEnvironment(simulator)
        permitted = permitted || (url.scheme == "http" && url.host == "127.0.0.1" && url.port == 3101)
        #endif
        guard permitted else { throw LocalError.invalidProtocol }
        self.base = url
        let config = URLSessionConfiguration.ephemeral
        config.httpCookieStorage = nil; config.httpShouldSetCookies = false; config.urlCache = nil
        config.requestCachePolicy = .reloadIgnoringLocalAndRemoteCacheData
        config.timeoutIntervalForRequest = 25; config.timeoutIntervalForResource = 30
        config.httpMaximumConnectionsPerHost = 2
        session = URLSession(configuration: config, delegate: NoRedirect(), delegateQueue: nil)
    }
    func login(email: String, password: String) async throws {
        let (_, response) = try await raw("/api/session", method: "POST", bytes: encode(JSON.object(["email": .s(email), "password": .s(password)])))
        let headers = response.allHeaderFields.reduce(into: [String: String]()) { result, pair in if let key = pair.key as? String, let value = pair.value as? String { result[key] = value } }
        guard let token = HTTPCookie.cookies(withResponseHeaderFields: headers, for: base).first(where: { $0.name == "crm_session" }) else { throw LocalError.invalidProtocol }
        cookie = "crm_session=" + token.value
    }
    func call<T: Decodable>(_ path: String, method: String = "GET", body: JSON? = nil, context: String? = nil) async throws -> T {
        let (data, _) = try await raw("/api/mobile/v1" + path, method: method, bytes: body.map { try encode($0) }, context: context)
        return try decode(T.self, data)
    }
    func operation(_ bytes: Data, context: String) async throws -> Receipt {
        let (data, _) = try await raw("/api/mobile/v1/operations", method: "POST", bytes: bytes, context: context)
        #if DEBUG
        if dropNextOperationResponse { dropNextOperationResponse = false; throw LocalError.lostResponse }
        #endif
        return try decode(Receipt.self, data)
    }
    func verifyAuthority(_ boot: Bootstrap) async throws {
        let (data, _) = try await raw("/api/me", method: "GET")
        let identity = try decode(JSON.self, data)
        guard identity["user"]["id"].text == boot.actor_user_id,
              identity["organization"]["id"].text == boot.organization_id,
              identity["organization"]["workspace_mode"].text == "operational" else { throw LocalError.locked }
    }
    func logout() async { _ = try? await raw("/api/session", method: "DELETE") }
    private func raw(_ path: String, method: String, bytes: Data? = nil, context: String? = nil) async throws -> (Data, HTTPURLResponse) {
        guard let url = URL(string: path, relativeTo: base)?.absoluteURL, url.host == base.host, url.port == base.port, url.scheme == base.scheme else { throw LocalError.invalidProtocol }
        var request = URLRequest(url: url); request.httpMethod = method; request.httpBody = bytes
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.setValue("no-store", forHTTPHeaderField: "Cache-Control")
        if !cookie.isEmpty { request.setValue(cookie, forHTTPHeaderField: "Cookie") }
        if let context { request.setValue(context, forHTTPHeaderField: "X-Mobile-Context") }
        let data: Data, response: URLResponse
        #if DEBUG
        if let responseForTesting { (data, response) = try await responseForTesting(request) }
        else { (data, response) = try await session.data(for: request) }
        #else
        (data, response) = try await session.data(for: request)
        #endif
        guard let response = response as? HTTPURLResponse else { throw LocalError.invalidProtocol }
        guard (200..<300).contains(response.statusCode) else {
            let code = (try? decode(JSON.self, data)["error"].text) ?? "invalid_response"
            throw APIError(status: response.statusCode, code: code.isEmpty ? "invalid_response" : code)
        }
        return (data, response)
    }
    static func cursor(_ value: String) -> String {
        var allowed = CharacterSet.urlQueryAllowed; allowed.remove(charactersIn: "&+=?#")
        return value.addingPercentEncoding(withAllowedCharacters: allowed) ?? ""
    }
}
