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
    @MainActor func testRepeatedReadOnlyRefreshBeyondGenerationCapacity() throws {
        continueAfterFailure = false
        let app = XCUIApplication(); app.launchArguments = ["--synthetic-keychain"]; app.launch()
        XCTAssertTrue(app.staticTexts["syntheticBanner"].waitForExistence(timeout: 20))
        if app.buttons["signIn"].exists {
            app.textFields["email"].tap(); app.textFields["email"].typeText("agent@mobile.test")
            app.secureTextFields["password"].tap(); app.secureTextFields["password"].typeText("Mobile-demo-only-123!")
            app.buttons["signIn"].tap()
        }
        XCTAssertTrue(app.tabBars.buttons["Settings"].waitForExistence(timeout: 60))
        app.tabBars.buttons["Settings"].tap(); setSwitch(app.switches["offlineToggle"], to: false)
        let lastSync = app.staticTexts.matching(NSPredicate(format: "label BEGINSWITH 'Last complete sync:'")).firstMatch
        for _ in 0..<5 {
            let previous = lastSync.label
            let ready = NSPredicate(format: "enabled == true")
            expectation(for: ready, evaluatedWith: app.buttons["sync"]); waitForExpectations(timeout: 60)
            app.buttons["sync"].tap()
            expectation(for: NSPredicate(format: "label != %@", previous), evaluatedWith: lastSync)
            waitForExpectations(timeout: 60)
            XCTAssertTrue(app.staticTexts["statusMessage"].label.contains("Synced. Complete downloaded workspace"), app.staticTexts["statusMessage"].label)
        }
        XCTAssertTrue(app.staticTexts["100 people available offline"].exists)
        let counts = app.staticTexts["coverageCounts"].label.replacingOccurrences(of: ",", with: "").split(whereSeparator: { !$0.isNumber }).compactMap { Int($0) }
        XCTAssertEqual(counts.count, 2); XCTAssertGreaterThanOrEqual(counts[0], 1000); XCTAssertGreaterThanOrEqual(counts[1], 1000)
        let proof = XCTAttachment(screenshot: app.screenshot()); proof.name = "five-successive-refreshes-with-complete-coverage"; proof.lifetime = .keepAlways; add(proof)
        // Finish paused so leaving the demo installed does not consume test generations.
        setSwitch(app.switches["offlineToggle"], to: true)
        app.tabBars.buttons["Saved work"].tap()
        let queueProof = XCTAttachment(screenshot: app.screenshot()); queueProof.name = "saved-work-with-collapsed-sync-details"; queueProof.lifetime = .keepAlways; add(queueProof)
    }

    #if MOBILE002_QA
    @MainActor func testMobile002NativeOfflineEditTerminateRelaunchAndSynchronizeReservedPerson001() throws {
        continueAfterFailure = false
        let app = XCUIApplication(); app.launchArguments = ["--synthetic-keychain", "--mobile002-qa-person-id", "f40f5132-9822-4bdf-ba34-76affb181195", "--mobile002-qa-note-id", "bfb73b8f-4036-49dc-8cb0-a1963babee13"]; app.launch()
        XCTAssertTrue(app.staticTexts["syntheticBanner"].waitForExistence(timeout: 20))
        if app.buttons["signIn"].exists {
            app.textFields["email"].tap(); app.textFields["email"].typeText("agent@mobile.test")
            app.secureTextFields["password"].tap(); app.secureTextFields["password"].typeText("Mobile-demo-only-123!")
            app.buttons["signIn"].tap()
        }
        XCTAssertTrue(app.tabBars.buttons["Settings"].waitForExistence(timeout: 80))
        app.tabBars.buttons["Settings"].tap()
        let stage = app.staticTexts["qaFixtureStage"]
        XCTAssertTrue(stage.waitForExistence(timeout: 10))
        setSwitch(app.switches["offlineToggle"], to: false); app.buttons["sync"].tap()
        expectation(for: NSPredicate(format: "label CONTAINS 'Synced. Complete downloaded workspace'"), evaluatedWith: app.staticTexts["statusMessage"])
        waitForExpectations(timeout: 120)
        expectation(for: NSPredicate(format: "enabled == true"), evaluatedWith: app.buttons["loadQANoteFixture"])
        waitForExpectations(timeout: 60); app.buttons["loadQANoteFixture"].tap()
        expectation(for: NSPredicate(format: "label == %@", "injection committed"), evaluatedWith: stage)
        waitForExpectations(timeout: 30)
        XCTAssertTrue(app.staticTexts["100 people available offline"].waitForExistence(timeout: 240))
        app.tabBars.buttons["People"].tap(); let search = app.searchFields.firstMatch; search.tap(); search.typeText("001")
        let person = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'person_' ")).firstMatch
        XCTAssertTrue(person.waitForExistence(timeout: 15)); person.tap()
        var edit = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'editNote_' ")).firstMatch
        XCTAssertTrue(edit.waitForExistence(timeout: 60), "QA bootstrap loads this exact server-backed editable note")
        app.tabBars.buttons["Settings"].tap(); setSwitch(app.switches["offlineToggle"], to: true); app.tabBars.buttons["People"].tap()
        // Return to the already loaded Person screen after toggling sync state.
        let selected = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'person_' ")).firstMatch; if selected.exists { selected.tap() }
        edit = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'editNote_' ")).firstMatch
        XCTAssertTrue(edit.waitForExistence(timeout: 10))
        edit.tap(); let editor = app.textViews["composerText"]; XCTAssertTrue(editor.waitForExistence(timeout: 10)); editor.tap(); editor.typeText(" · offline iOS QA")
        XCTAssertTrue(app.staticTexts["draftStatus"].label.contains("Draft saved on device")); app.buttons["saveAction"].tap()
        app.tabBars.buttons["Saved work"].tap(); XCTAssertTrue(app.staticTexts["queueCount"].label.contains("1 pending"))
        let pending = XCTAttachment(screenshot: app.screenshot()); pending.name = "mobile002-offline-edit-pending"; pending.lifetime = .keepAlways; add(pending)
        app.terminate(); app.launch(); XCTAssertTrue(app.tabBars.buttons["Saved work"].waitForExistence(timeout: 30)); app.tabBars.buttons["Saved work"].tap()
        XCTAssertTrue(app.staticTexts["queueCount"].label.contains("1 pending"))
        app.tabBars.buttons["Settings"].tap(); setSwitch(app.switches["offlineToggle"], to: false); app.buttons["sync"].tap()
        app.tabBars.buttons["Saved work"].tap(); expectation(for: NSPredicate(format: "label BEGINSWITH '0 pending'"), evaluatedWith: app.staticTexts["queueCount"]); waitForExpectations(timeout: 120)
    }
    #endif

}
