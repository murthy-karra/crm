import Foundation
import Security
import Darwin

// mach_continuous_time advances during device sleep; systemUptime is not the lease clock.
func continuousSeconds() -> Double {
    var info = mach_timebase_info_data_t()
    mach_timebase_info(&info)
    return Double(mach_continuous_time()) * Double(info.numer) / Double(info.denom) / 1_000_000_000
}

struct ClockSample: Codable, Equatable, Sendable {
    let boot: String, uptime: Double, wall: Double
    static func current() throws -> ClockSample {
        var size = 0
        guard sysctlbyname("kern.bootsessionuuid", nil, &size, nil, 0) == 0, size > 1 else { throw LocalError.locked }
        var bytes = [CChar](repeating: 0, count: size)
        guard sysctlbyname("kern.bootsessionuuid", &bytes, &size, nil, 0) == 0 else { throw LocalError.locked }
        return ClockSample(boot: String(cString: bytes), uptime: continuousSeconds(), wall: Date().timeIntervalSince1970)
    }
}
struct Lease: Codable, Sendable {
    let anchor: ClockSample, expiresAfter: Double
    var last: ClockSample
    init(bootstrap: Bootstrap, clock: ClockSample) throws {
        anchor = clock; last = clock
        expiresAfter = min(7 * 86400, try date(bootstrap.offline_access_expires_at).timeIntervalSince(date(bootstrap.server_time)))
        guard expiresAfter > 0 else { throw LocalError.locked }
    }
    mutating func validate(_ now: ClockSample) throws {
        guard now.boot == anchor.boot, now.uptime >= last.uptime, now.uptime >= anchor.uptime,
              now.uptime - anchor.uptime < expiresAfter,
              now.wall >= last.wall - 2 else { throw LocalError.locked }
        // Monotonic time governs duration. A wall-clock rollback fails closed, even within the same boot.
        last = now
    }
}
struct Credential: Codable {
    var cookie: String
    let baseURL: String, bootstrap: Bootstrap
    var lease: Lease, signedOut: Bool
}
final class SecureStorage {
    let synthetic: Bool
    private let service: String
    #if DEBUG
    var testDirectory: URL?
    #endif
    private func markerURL() throws -> URL {
        #if DEBUG
        if let testDirectory { return testDirectory.appendingPathComponent("access-lock") }
        #endif
        return try Self.directory(synthetic: synthetic).appendingPathComponent("access-lock")
    }
    init(synthetic: Bool = false, testingNamespace: String? = nil) {
        #if DEBUG && targetEnvironment(simulator)
        self.synthetic = synthetic
        #else
        self.synthetic = false
        #endif
        let base = self.synthetic ? "dev.crm.field.synthetic" : "dev.crm.field.protected"
        #if DEBUG
        service = testingNamespace.map { base + ".test." + $0 } ?? base
        #else
        service = base
        #endif
    }
    #if DEBUG
    var failCredentialWritesForTesting = false
    #endif
    func isLocked() throws -> Bool {
        let marker = try markerURL()
        return FileManager.default.fileExists(atPath: marker.path)
    }
    func setLocked() throws {
        let marker = try markerURL()
        try Data("locked".utf8).write(to: marker, options: [.atomic, .completeFileProtection])
        try flush(marker)
        try flush(marker.deletingLastPathComponent())
    }
    func clearLockAfterAuthorization() throws {
        let marker = try markerURL()
        if FileManager.default.fileExists(atPath: marker.path) { try FileManager.default.removeItem(at: marker) }
        try flush(marker.deletingLastPathComponent())
    }
    private func flush(_ url: URL) throws {
        let fd = Darwin.open(url.path, O_RDONLY)
        guard fd >= 0 else { throw LocalError.storage }
        defer { Darwin.close(fd) }
        guard fsync(fd) == 0 else { throw LocalError.storage }
    }
    func removeCredential() throws {
        let status = SecItemDelete(base("credential") as CFDictionary)
        guard status == errSecSuccess || status == errSecItemNotFound else { throw LocalError.unavailableKey }
    }
    func read(_ account: String) throws -> Data? {
        var query = base(account); query[kSecReturnData as String] = true; query[kSecMatchLimit as String] = kSecMatchLimitOne
        var value: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &value)
        if status == errSecItemNotFound { return nil }
        guard status == errSecSuccess, let data = value as? Data else { throw LocalError.unavailableKey }
        return data
    }
    func write(_ account: String, _ data: Data) throws {
        #if targetEnvironment(simulator)
        // Simulator cannot establish physical passcode enforcement. Only the visible,
        // explicitly opted-in synthetic Debug mode may persist credentials/keys here.
        guard synthetic else { throw LocalError.unavailableKey }
        #endif
        #if DEBUG
        if account == "credential" && failCredentialWritesForTesting { throw LocalError.unavailableKey }
        #endif
        // Update in place: never delete the last working credential/key before a replacement is durable.
        let access = synthetic ? kSecAttrAccessibleWhenUnlockedThisDeviceOnly : kSecAttrAccessibleWhenPasscodeSetThisDeviceOnly
        let attributes: [String: Any] = [kSecValueData as String: data, kSecAttrAccessible as String: access]
        let status = SecItemUpdate(base(account) as CFDictionary, attributes as CFDictionary)
        if status == errSecItemNotFound {
            var query = base(account); attributes.forEach { query[$0] = $1 }
            guard SecItemAdd(query as CFDictionary, nil) == errSecSuccess else { throw LocalError.unavailableKey }
        } else if status != errSecSuccess { throw LocalError.unavailableKey }
    }
    func key(for identity: String, existingFile: Bool) throws -> Data {
        if let data = try read("key." + identity) { guard data.count == 32 else { throw LocalError.unavailableKey }; return data }
        // Lost keys never cause an existing database to be recreated with a new key.
        guard !existingFile else { throw LocalError.unavailableKey }
        var data = Data(count: 32)
        guard data.withUnsafeMutableBytes({ SecRandomCopyBytes(kSecRandomDefault, 32, $0.baseAddress!) }) == errSecSuccess else { throw LocalError.unavailableKey }
        try write("key." + identity, data); return data
    }
    private func base(_ account: String) -> [String: Any] {
        [kSecClass as String: kSecClassGenericPassword, kSecAttrService as String: service,
         kSecAttrAccount as String: account, kSecAttrSynchronizable as String: false]
    }
    static func directory(synthetic: Bool) throws -> URL {
        var url = try FileManager.default.url(for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
            .appendingPathComponent(synthetic ? "SyntheticFieldCRM" : "FieldCRM", isDirectory: true)
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true,
                                              attributes: [.protectionKey: FileProtectionType.complete])
        var values = URLResourceValues(); values.isExcludedFromBackup = true
        try url.setResourceValues(values)
        return url
    }
}
