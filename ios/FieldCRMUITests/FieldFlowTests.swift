import XCTest

final class FieldFlowTests: XCTestCase {
    @MainActor func setSwitch(_ element: XCUIElement, to value: Bool) {
        let expected = value ? "1" : "0"
        if element.value as? String != expected {
            // SwiftUI exposes the whole Form row as the switch; target its actual trailing control.
            element.coordinate(withNormalizedOffset: CGVector(dx: 0.92, dy: 0.5)).tap()
        }
        XCTAssertEqual(element.value as? String, expected)
    }
    @MainActor func testNativeOfflineNoteTaskTerminateRelaunchAndSynchronize() throws {
        continueAfterFailure = false
        let app = XCUIApplication(); app.launchArguments = ["--synthetic-keychain"]; app.launch()
        XCTAssertTrue(app.staticTexts["syntheticBanner"].waitForExistence(timeout: 20))
        if app.buttons["signIn"].exists {
            app.textFields["email"].tap(); app.textFields["email"].typeText("agent@mobile.test")
            app.secureTextFields["password"].tap(); app.secureTextFields["password"].typeText("Mobile-demo-only-123!")
            app.buttons["signIn"].tap()
        }
        XCTAssertTrue(app.tabBars.buttons["Settings"].waitForExistence(timeout: 60))
        app.tabBars.buttons["Settings"].tap()
        let toggle = app.switches["offlineToggle"]
        setSwitch(toggle, to: false)
        app.buttons["sync"].tap()
        XCTAssertTrue(app.staticTexts["100 people available offline"].waitForExistence(timeout: 240))
        app.tabBars.buttons["Saved work"].tap()
        expectation(for: NSPredicate(format: "label BEGINSWITH '0 pending'"), evaluatedWith: app.staticTexts["queueCount"])
        waitForExpectations(timeout: 90)
        app.tabBars.buttons["Settings"].tap()
        setSwitch(toggle, to: true)
        XCTAssertTrue(app.staticTexts["Working offline"].exists)
        app.tabBars.buttons["People"].tap()
        let search = app.searchFields.firstMatch; search.tap(); search.typeText("080")
        let person = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'person_'")).firstMatch
        XCTAssertTrue(person.waitForExistence(timeout: 10)); person.tap()
        let label = "iOS UI " + UUID().uuidString.prefix(8)
        app.buttons["addNote"].tap()
        let editor = app.textViews["composerText"]; XCTAssertTrue(editor.waitForExistence(timeout: 5)); editor.tap(); editor.typeText(label)
        XCTAssertTrue(app.staticTexts["draftStatus"].label.contains("Draft saved on device"))
        app.buttons["saveAction"].tap()
        XCTAssertTrue(app.staticTexts[label].waitForExistence(timeout: 10))
        app.buttons["createTask"].tap(); app.textViews["composerText"].tap(); app.textViews["composerText"].typeText(label + " task")
        app.swipeUp(); app.buttons["saveAction"].tap()
        let complete = app.buttons["Complete saved task"].firstMatch
        for _ in 0..<8 {
            if complete.exists && complete.frame.midY > 260 && complete.frame.midY < app.frame.height - 170 { break }
            let upward = !complete.exists || complete.frame.midY >= app.frame.height - 170
            app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: upward ? 0.70 : 0.45))
                .press(forDuration: 0.05, thenDragTo: app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: upward ? 0.55 : 0.60)))
        }
        XCTAssertTrue(complete.exists); XCTAssertGreaterThan(complete.frame.midY, 260); complete.tap()
        app.tabBars.buttons["Saved work"].tap()
        XCTAssertTrue(app.staticTexts["queueCount"].label.contains("3 pending"), app.staticTexts["queueCount"].label)
        let pending = XCTAttachment(screenshot: app.screenshot()); pending.name = "offline-note-task-and-completion"; pending.lifetime = .keepAlways; add(pending)
        app.terminate(); app.launch()
        XCTAssertTrue(app.tabBars.buttons["Saved work"].waitForExistence(timeout: 30)); app.tabBars.buttons["Saved work"].tap()
        XCTAssertTrue(app.staticTexts["queueCount"].label.contains("3 pending"), app.staticTexts["queueCount"].label)
        XCTAssertTrue(app.staticTexts[label].exists)
        app.tabBars.buttons["Settings"].tap()
        XCTAssertEqual(app.switches["offlineToggle"].value as? String, "1")
        setSwitch(app.switches["offlineToggle"], to: false); app.buttons["sync"].tap()
        app.tabBars.buttons["Saved work"].tap()
        let synced = NSPredicate(format: "label BEGINSWITH '0 pending'")
        expectation(for: synced, evaluatedWith: app.staticTexts["queueCount"])
        waitForExpectations(timeout: 120)
        let attachment = XCTAttachment(screenshot: app.screenshot()); attachment.name = "synced-native-queue"; attachment.lifetime = .keepAlways; add(attachment)
        app.tabBars.buttons["Settings"].tap(); app.buttons["signOut"].tap(); app.buttons["Sign out and protect saved work"].tap()
        XCTAssertTrue(app.buttons["signIn"].waitForExistence(timeout: 20))
        XCTAssertFalse(app.tabBars.buttons["People"].exists)
    }
}
