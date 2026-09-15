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

    @MainActor func testMobile007RequestedIntentSurvivesClearedSearchAndRelaunch() throws {
        continueAfterFailure = false
        let app = XCUIApplication(); app.launchArguments = ["--synthetic-keychain", "uiMobile007Requested"]; app.launch()
        func tab(_ name: String) {
            if app.buttons["Close"].exists { app.buttons["Close"].tap() }
            if app.keyboards.buttons["Search"].exists { app.keyboards.buttons["Search"].tap() }
            XCTAssertTrue(app.tabBars.buttons[name].waitForExistence(timeout: 20))
            app.tabBars.buttons[name].tap()
        }
        func finishSync() {
            tab("Settings")
            XCTAssertTrue(app.switches["offlineToggle"].waitForExistence(timeout: 10))
            setSwitch(app.switches["offlineToggle"], to: false)
            let sync = app.buttons["sync"]
            expectation(for: NSPredicate(format: "enabled == true"), evaluatedWith: sync)
            waitForExpectations(timeout: 180)
            sync.tap()
            expectation(for: NSPredicate(format: "label BEGINSWITH 'Synced. Complete downloaded workspace'"), evaluatedWith: app.staticTexts["statusMessage"])
            waitForExpectations(timeout: 240)
        }
        func filterDownloaded(_ term: String) {
            let field = app.searchFields.firstMatch
            for _ in 0..<8 where !field.exists { app.swipeDown() }
            XCTAssertTrue(field.waitForExistence(timeout: 10)); field.tap()
            if app.buttons["Clear text"].exists { app.buttons["Clear text"].tap() }
            field.typeText(term)
            if app.keyboards.buttons["Search"].exists { app.keyboards.buttons["Search"].tap() }
        }
        func searchOrganization(_ term: String) {
            let field = app.textFields["Name, exact email or phone"]
            for _ in 0..<8 where !field.exists { app.swipeUp() }
            XCTAssertTrue(field.waitForExistence(timeout: 10))
            if app.buttons["Clear"].exists { app.buttons["Clear"].tap() }
            field.tap(); field.typeText(term)
            app.buttons["Search"].tap()
        }
        XCTAssertTrue(app.staticTexts["syntheticBanner"].waitForExistence(timeout: 20))
        if app.buttons["signIn"].exists {
            app.textFields["email"].tap(); app.textFields["email"].typeText("agent@mobile.test")
            app.secureTextFields["password"].tap(); app.secureTextFields["password"].typeText("Mobile-demo-only-123!")
            app.buttons["signIn"].tap()
        }
        finishSync()
        tab("People"); filterDownloaded("Remote Prospect")
        let cancel = app.buttons["cancelRequested_676d4fbb-7cb1-43c7-980f-e8769ef68a64"]
        if cancel.waitForExistence(timeout: 3) { cancel.tap() }
        finishSync()
        XCTAssertTrue(app.staticTexts["100 people available offline"].exists)
        setSwitch(app.switches["offlineToggle"], to: true)
        tab("People"); filterDownloaded("Remote Prospect")
        searchOrganization("Discovery Twin")
        XCTAssertTrue(app.staticTexts["twin.one@synthetic.test"].waitForExistence(timeout: 20))
        XCTAssertTrue(app.staticTexts["twin.two@synthetic.test"].exists)
        searchOrganization("twin.two@synthetic.test")
        XCTAssertTrue(app.staticTexts["twin.two@synthetic.test"].waitForExistence(timeout: 20))
        XCTAssertFalse(app.staticTexts["twin.one@synthetic.test"].exists)
        searchOrganization("+12025550101")
        XCTAssertTrue(app.staticTexts["twin.one@synthetic.test"].waitForExistence(timeout: 20))
        XCTAssertFalse(app.staticTexts["twin.two@synthetic.test"].exists)
        searchOrganization("Remote Prospect")
        XCTAssertTrue(app.staticTexts["Remote Prospect"].waitForExistence(timeout: 20))
        XCTAssertTrue(app.buttons["Save offline"].firstMatch.waitForExistence(timeout: 20))
        app.buttons["Save offline"].firstMatch.tap(); app.buttons["Clear"].tap()
        for _ in 0..<5 where !cancel.exists { app.swipeDown() }
        let requestedProof = XCTAttachment(screenshot: app.screenshot()); requestedProof.name = "requested-after-clear"; requestedProof.lifetime = .keepAlways; add(requestedProof)
        XCTAssertTrue(cancel.waitForExistence(timeout: 10))
        app.terminate(); app.launch()
        tab("People"); filterDownloaded("Remote Prospect")
        XCTAssertTrue(cancel.waitForExistence(timeout: 10))
        XCTAssertTrue(app.staticTexts["Person 676d4fbb"].exists)
        app.buttons["Retry requested downloads"].tap()
        finishSync()
        XCTAssertTrue(app.staticTexts["101 people available offline"].exists)
        setSwitch(app.switches["offlineToggle"], to: true)
        tab("People"); filterDownloaded("Remote Prospect")
        let remote = app.buttons["person_676d4fbb-7cb1-43c7-980f-e8769ef68a64"]
        XCTAssertTrue(remote.waitForExistence(timeout: 10))
        XCTAssertTrue(app.staticTexts["Last synced: pinned"].exists)
        remote.tap(); XCTAssertTrue(app.buttons["addNote"].waitForExistence(timeout: 10))
        app.buttons["addNote"].tap()
        let note = "Mobile007 offline proof " + UUID().uuidString.prefix(8)
        app.textViews["composerText"].tap(); app.textViews["composerText"].typeText(note)
        app.buttons["saveAction"].tap()
        tab("Saved work")
        XCTAssertTrue(app.staticTexts[note].waitForExistence(timeout: 10))
        app.terminate(); app.launch(); tab("Saved work")
        XCTAssertTrue(app.staticTexts[note].waitForExistence(timeout: 20))
        XCTAssertTrue(app.staticTexts["queueCount"].label.contains("1 pending"))
        let proof = XCTAttachment(screenshot: app.screenshot()); proof.name = "mobile007-downloaded-person-offline-note-after-restart"; proof.lifetime = .keepAlways; add(proof)
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

    #if MOBILE003_QA
    @MainActor func testMobile003NativeOfflineContactTerminatesRelaunchesAndReceivesReceipt() throws {
        continueAfterFailure = false
        let app = XCUIApplication(); app.launchArguments = ["--synthetic-keychain"]; app.launch()
        XCTAssertTrue(app.staticTexts["syntheticBanner"].waitForExistence(timeout: 20))
        if app.buttons["signIn"].exists {
            app.textFields["email"].tap(); app.textFields["email"].typeText("agent@mobile.test")
            app.secureTextFields["password"].tap(); app.secureTextFields["password"].typeText("Mobile-demo-only-123!")
            app.buttons["signIn"].tap()
        }
        XCTAssertTrue(app.tabBars.buttons["Settings"].waitForExistence(timeout: 80))
        app.tabBars.buttons["Settings"].tap(); setSwitch(app.switches["offlineToggle"], to: false); app.buttons["sync"].tap()
        XCTAssertTrue(app.staticTexts["100 people available offline"].waitForExistence(timeout: 240))
        app.tabBars.buttons["Settings"].tap(); setSwitch(app.switches["offlineToggle"], to: true)
        app.tabBars.buttons["People"].tap(); let search = app.searchFields.firstMatch; search.tap(); search.typeText("001")
        let person = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'person_' ")).firstMatch
        XCTAssertTrue(person.waitForExistence(timeout: 15)); person.tap()
        let contact = app.buttons["logContact"]; XCTAssertTrue(contact.waitForExistence(timeout: 10)); contact.tap()
        XCTAssertTrue(app.staticTexts["Manual contact"].waitForExistence(timeout: 10))
        XCTAssertTrue(app.staticTexts["Calls made through the CRM already have a contact record. Use this form only for a manual interaction that already happened."].exists)
        XCTAssertTrue(app.staticTexts["contactDraftStatus"].label.contains("Draft saved on device"))
        app.buttons["saveContact"].tap()
        app.tabBars.buttons["Today"].tap(); XCTAssertTrue(app.staticTexts["pendingContactBadge"].waitForExistence(timeout: 15))
        app.tabBars.buttons["Saved work"].tap(); XCTAssertTrue(app.staticTexts["queueCount"].label.contains("1 pending"))
        let pending = XCTAttachment(screenshot: app.screenshot()); pending.name = "mobile003-offline-contact-pending"; pending.lifetime = .keepAlways; add(pending)
        app.terminate(); app.launch()
        XCTAssertTrue(app.tabBars.buttons["Saved work"].waitForExistence(timeout: 30)); app.tabBars.buttons["Saved work"].tap()
        XCTAssertTrue(app.staticTexts["queueCount"].label.contains("1 pending"))
        app.tabBars.buttons["Settings"].tap(); setSwitch(app.switches["offlineToggle"], to: false); app.buttons["sync"].tap()
        app.tabBars.buttons["Saved work"].tap()
        expectation(for: NSPredicate(format: "label BEGINSWITH '0 pending'"), evaluatedWith: app.staticTexts["queueCount"]); waitForExpectations(timeout: 180)
        let receipt = XCTAttachment(screenshot: app.screenshot()); receipt.name = "mobile003-contact-accepted-after-relaunch"; receipt.lifetime = .keepAlways; add(receipt)
        app.tabBars.buttons["Settings"].tap(); app.buttons["inspectMobile003Migration"].tap()
        XCTAssertTrue(app.staticTexts["qaMobile003MigrationStage"].label.contains("schema=6"))
    }
    #endif

    #if MOBILE004_UPGRADE_QA
    @MainActor func testMobile004NativeOfflineStageTerminatesRelaunchesAndSynchronizes() throws {
        continueAfterFailure = false
        let app = XCUIApplication(); app.launchArguments = ["--synthetic-keychain"]; app.launch()
        XCTAssertTrue(app.staticTexts["syntheticBanner"].waitForExistence(timeout: 20))
        XCTAssertTrue(app.tabBars.buttons["Settings"].waitForExistence(timeout: 60), "The current app must reopen the upgraded protected store")
        app.tabBars.buttons["Settings"].tap(); setSwitch(app.switches["offlineToggle"], to: false); app.buttons["sync"].tap()
        XCTAssertTrue(app.staticTexts["100 people available offline"].waitForExistence(timeout: 240))
        app.tabBars.buttons["People"].tap(); let search = app.searchFields.firstMatch; search.tap(); search.typeText("001")
        let person = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'person_' ")).firstMatch
        XCTAssertTrue(person.waitForExistence(timeout: 20)); person.tap()
        let change = app.buttons["changeStage"]
        XCTAssertTrue(change.waitForExistence(timeout: 20)); XCTAssertTrue(change.isEnabled, "A fully sealed catalog and qualified summary enable stage proposals")
        app.tabBars.buttons["Settings"].tap(); setSwitch(app.switches["offlineToggle"], to: true); app.tabBars.buttons["People"].tap()
        let selected = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'person_' ")).firstMatch; if selected.exists { selected.tap() }
        XCTAssertTrue(change.waitForExistence(timeout: 15)); change.tap()
        XCTAssertTrue(app.staticTexts["Downloaded server stage"].waitForExistence(timeout: 10))
        // The saved default is the server stage. This exercises the visible
        // no-op stage path; the focused real-API test separately proves a real
        // different-target transition and conflict recovery.
        app.buttons["saveStage"].tap()
        app.tabBars.buttons["Saved work"].tap(); XCTAssertTrue(app.staticTexts["queueCount"].label.contains("1 pending"))
        app.tabBars.buttons["People"].tap()
        let followUpPerson = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'person_' ")).firstMatch
        if followUpPerson.exists { followUpPerson.tap() }
        XCTAssertTrue(change.waitForExistence(timeout: 15)); change.tap()
        XCTAssertTrue(app.staticTexts["Downloaded server stage"].waitForExistence(timeout: 10))
        app.buttons["saveStage"].tap()
        app.tabBars.buttons["Saved work"].tap()
        let followUp = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'stageFollowUp_' ")).firstMatch
        XCTAssertTrue(followUp.waitForExistence(timeout: 15), "A second stage selection is saved as a visible follow-up draft, not queued work")
        let pending = XCTAttachment(screenshot: app.screenshot()); pending.name = "mobile004-stage-pending-after-offline-save"; pending.lifetime = .keepAlways; add(pending)
        app.terminate(); app.launch()
        XCTAssertTrue(app.tabBars.buttons["Saved work"].waitForExistence(timeout: 30)); app.tabBars.buttons["Saved work"].tap()
        XCTAssertTrue(app.staticTexts["queueCount"].label.contains("1 pending"), "The immutable stage envelope survives a process restart")
        app.tabBars.buttons["Settings"].tap(); setSwitch(app.switches["offlineToggle"], to: false); app.buttons["sync"].tap()
        app.tabBars.buttons["Saved work"].tap()
        expectation(for: NSPredicate(format: "label BEGINSWITH '0 pending'"), evaluatedWith: app.staticTexts["queueCount"]); waitForExpectations(timeout: 180)
        app.tabBars.buttons["Settings"].tap(); setSwitch(app.switches["offlineToggle"], to: true); app.tabBars.buttons["Saved work"].tap()
        XCTAssertTrue(followUp.waitForExistence(timeout: 15)); followUp.tap()
        XCTAssertTrue(app.staticTexts["Downloaded server stage"].waitForExistence(timeout: 10))
        app.buttons["saveStage"].tap(); app.tabBars.buttons["Saved work"].tap()
        XCTAssertTrue(app.staticTexts["queueCount"].label.contains("1 pending"), "A follow-up only enters the outbox after the user explicitly submits it")
        app.tabBars.buttons["Settings"].tap(); setSwitch(app.switches["offlineToggle"], to: false); app.buttons["sync"].tap(); app.tabBars.buttons["Saved work"].tap()
        expectation(for: NSPredicate(format: "label BEGINSWITH '0 pending'"), evaluatedWith: app.staticTexts["queueCount"]); waitForExpectations(timeout: 180)
        let accepted = XCTAttachment(screenshot: app.screenshot()); accepted.name = "mobile004-stage-accepted-after-relaunch"; accepted.lifetime = .keepAlways; add(accepted)
        app.tabBars.buttons["Settings"].tap(); setSwitch(app.switches["offlineToggle"], to: true)
    }
    #endif

    #if MOBILE006_QA
    @MainActor func testMobile006MetadataOfflineRestartAndSynchronize() throws {
        continueAfterFailure = false
        let app = XCUIApplication(); app.launchArguments = ["--synthetic-keychain"]; app.launch()
        XCTAssertTrue(app.staticTexts["syntheticBanner"].waitForExistence(timeout: 20))
        if app.buttons["signIn"].exists {
            app.textFields["email"].tap(); app.textFields["email"].typeText("agent@mobile.test")
            app.secureTextFields["password"].tap(); app.secureTextFields["password"].typeText("Mobile-demo-only-123!")
            app.buttons["signIn"].tap()
        }
        XCTAssertTrue(app.tabBars.buttons["Settings"].waitForExistence(timeout: 90))
        app.tabBars.buttons["Settings"].tap(); setSwitch(app.switches["offlineToggle"], to: false); app.buttons["sync"].tap()
        XCTAssertTrue(app.staticTexts["100 people available offline"].waitForExistence(timeout: 240))
        app.tabBars.buttons["Settings"].tap(); setSwitch(app.switches["offlineToggle"], to: true)
        app.tabBars.buttons["People"].tap()
        let person = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'person_' ")).firstMatch
        XCTAssertTrue(person.waitForExistence(timeout: 20)); person.tap()
        let edit = app.buttons["editMetadata"]
        XCTAssertTrue(edit.waitForExistence(timeout: 20), "A complete sealed metadata catalog and component enable editing")
        edit.tap()
        let tag = app.switches.matching(NSPredicate(format: "identifier BEGINSWITH 'metadataTag_' ")).firstMatch
        XCTAssertTrue(tag.waitForExistence(timeout: 15)); tag.tap()
        let text = app.textFields.matching(NSPredicate(format: "identifier BEGINSWITH 'metadataText_' ")).firstMatch
        XCTAssertTrue(text.waitForExistence(timeout: 15)); text.tap(); text.typeText("iOS offline metadata")
        let number = app.textFields.matching(NSPredicate(format: "identifier BEGINSWITH 'metadataNumber_' ")).firstMatch
        XCTAssertTrue(number.waitForExistence(timeout: 15)); number.tap(); number.typeText("123.4500")
        // The fixture's localized field labels are server-owned. The focused
        // storage test validates date and choice values; this installed UI
        // journey exercises tag/text/number saving, restart and sync.
        XCTAssertTrue(app.staticTexts["metadataDraftStatus"].label.contains("Draft saved on device"))
        app.buttons["saveMetadata"].tap()
        app.tabBars.buttons["Saved work"].tap()
        XCTAssertTrue(app.staticTexts["queueCount"].label.contains("1 pending"), app.staticTexts["queueCount"].label)
        let pending = XCTAttachment(screenshot: app.screenshot()); pending.name = "mobile006-metadata-offline-pending"; pending.lifetime = .keepAlways; add(pending)
        app.terminate(); app.launch()
        XCTAssertTrue(app.tabBars.buttons["Saved work"].waitForExistence(timeout: 30)); app.tabBars.buttons["Saved work"].tap()
        XCTAssertTrue(app.staticTexts["queueCount"].label.contains("1 pending"), "The metadata envelope survives a full app restart")
        app.tabBars.buttons["Settings"].tap(); setSwitch(app.switches["offlineToggle"], to: false); app.buttons["sync"].tap()
        app.tabBars.buttons["Saved work"].tap()
        expectation(for: NSPredicate(format: "label BEGINSWITH '0 pending'"), evaluatedWith: app.staticTexts["queueCount"])
        waitForExpectations(timeout: 180)
        let accepted = XCTAttachment(screenshot: app.screenshot()); accepted.name = "mobile006-metadata-accepted"; accepted.lifetime = .keepAlways; add(accepted)
        app.tabBars.buttons["Settings"].tap(); setSwitch(app.switches["offlineToggle"], to: true)
    }
    @MainActor func testMobile006MetadataConflictShowsValuesAndSavesRevisedProposal() throws {
        continueAfterFailure = false
        let app = XCUIApplication(); app.launchArguments = ["--synthetic-keychain"]; app.launch()
        XCTAssertTrue(app.staticTexts["syntheticBanner"].waitForExistence(timeout: 20))
        if app.buttons["signIn"].exists {
            app.textFields["email"].tap(); app.textFields["email"].typeText("agent@mobile.test")
            app.secureTextFields["password"].tap(); app.secureTextFields["password"].typeText("Mobile-demo-only-123!")
            app.buttons["signIn"].tap()
        }
        XCTAssertTrue(app.tabBars.buttons["Settings"].waitForExistence(timeout: 90))
        app.tabBars.buttons["Settings"].tap(); setSwitch(app.switches["offlineToggle"], to: false); app.buttons["sync"].tap()
        XCTAssertTrue(app.staticTexts["100 people available offline"].waitForExistence(timeout: 240))
        app.tabBars.buttons["Settings"].tap(); setSwitch(app.switches["offlineToggle"], to: true)
        let prepare = app.buttons["qaPrepareMetadataConflict"]
        XCTAssertTrue(prepare.waitForExistence(timeout: 10))
        expectation(for: NSPredicate(format: "isEnabled == 1"), evaluatedWith: prepare)
        waitForExpectations(timeout: 30)
        prepare.tap()
        XCTAssertTrue(app.staticTexts["qaMetadataConflictStage"].waitForExistence(timeout: 10))
        XCTAssertEqual(app.staticTexts["qaMetadataConflictStage"].label, "local metadata conflict ready")
        app.tabBars.buttons["Saved work"].tap()
        let review = app.buttons["Review conflict"].firstMatch
        for _ in 0..<6 where !review.exists { app.swipeUp() }
        XCTAssertTrue(review.waitForExistence(timeout: 20)); review.tap()
        for _ in 0..<4 where !app.staticTexts["metadataComparison_baseline"].exists { app.swipeUp() }
        let baseline = app.staticTexts["metadataComparison_baseline"], proposed = app.staticTexts["metadataComparison_proposed"], current = app.staticTexts["metadataComparison_current"]
        XCTAssertTrue(baseline.waitForExistence(timeout: 10)); XCTAssertTrue(proposed.exists); XCTAssertTrue(current.exists)
        XCTAssertTrue(baseline.label.contains("tags:"), baseline.label)
        XCTAssertTrue(proposed.label.contains("tag:"), proposed.label)
        XCTAssertTrue(current.label.contains("tags:"), current.label)
        let reviewed = XCTAttachment(screenshot: app.screenshot()); reviewed.name = "mobile006-metadata-three-way-values"; reviewed.lifetime = .keepAlways; add(reviewed)
        let revised = app.buttons["Prepare revised proposal against current values"]
        XCTAssertTrue(revised.waitForExistence(timeout: 10)); revised.tap()
        XCTAssertTrue(app.staticTexts["metadataDraftStatus"].label.contains("Revised proposal saved on device"))
        for _ in 0..<6 where !app.buttons["saveMetadata"].exists { app.swipeUp() }
        XCTAssertTrue(app.buttons["saveMetadata"].exists, "The reset editor retains the revised proposal and controls")
        let saved = XCTAttachment(screenshot: app.screenshot()); saved.name = "mobile006-revised-metadata-proposal"; saved.lifetime = .keepAlways; add(saved)
    }
    #endif

    #if MOBILE006_UPGRADE_QA
    @MainActor func testMobile006OpensActualInstalledMobile005StoreAndSavesMetadata() throws {
        continueAfterFailure = false
        let app = XCUIApplication(); app.launchArguments = ["--synthetic-keychain"]; app.launch()
        XCTAssertTrue(app.staticTexts["syntheticBanner"].waitForExistence(timeout: 20))
        XCTAssertTrue(app.tabBars.buttons["Settings"].waitForExistence(timeout: 60), "The in-place app must restore the Mobile005 protected store and key")
        app.tabBars.buttons["Settings"].tap(); app.buttons["inspectMobile006Upgrade"].tap()
        let inventory = app.staticTexts["qaMobile006UpgradeStage"]
        XCTAssertTrue(inventory.label.contains("schema=9"), inventory.label)
        XCTAssertTrue(inventory.label.contains("people=100"), inventory.label)
        XCTAssertTrue(inventory.label.contains("receipts=") && !inventory.label.contains("receipts=0"), inventory.label)
        XCTAssertTrue(inventory.label.contains("drafts="), inventory.label)
        // Download the new metadata representation without replacing the old
        // protected work, then save a new metadata envelope after upgrade.
        setSwitch(app.switches["offlineToggle"], to: false); app.buttons["sync"].tap()
        XCTAssertTrue(app.staticTexts["100 people available offline"].waitForExistence(timeout: 240))
        app.tabBars.buttons["Settings"].tap(); setSwitch(app.switches["offlineToggle"], to: true); app.tabBars.buttons["People"].tap()
        let person = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'person_' ")).firstMatch
        XCTAssertTrue(person.waitForExistence(timeout: 20)); person.tap()
        XCTAssertTrue(app.buttons["editMetadata"].waitForExistence(timeout: 20)); app.buttons["editMetadata"].tap()
        XCTAssertTrue(app.buttons["stageMetadataUpgradeProof"].waitForExistence(timeout: 15)); app.buttons["stageMetadataUpgradeProof"].tap()
        for _ in 0..<6 where !app.buttons["saveMetadata"].exists { app.swipeUp() }
        XCTAssertTrue(app.buttons["saveMetadata"].exists, "The staged metadata proposal remains available to save after the installed-app upgrade")
        app.buttons["saveMetadata"].tap(); app.tabBars.buttons["Saved work"].tap()
        XCTAssertTrue(app.staticTexts["queueCount"].label.contains("pending"), app.staticTexts["queueCount"].label)
        let proof = XCTAttachment(screenshot: app.screenshot()); proof.name = "mobile006-new-metadata-after-installed-mobile005-upgrade"; proof.lifetime = .keepAlways; add(proof)
    }
    #endif

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

    @MainActor func testMobile002NativeSecondActorConflictReviewAndRevisedReceipt() throws {
        // This test intentionally reuses the already-authorized primary QA
        // installation on the isolated iPhone 17.  Bootstrap capacity is
        // bounded per actor, so it must not create another installation.
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchArguments = ["--synthetic-keychain", "--mobile002-qa-start-offline", "--mobile002-qa-person-id", "f40f5132-9822-4bdf-ba34-76affb181195", "--mobile002-qa-note-id", "bfb73b8f-4036-49dc-8cb0-a1963babee13"]
        app.launch()
        if app.buttons["Not Now"].waitForExistence(timeout: 2) { app.buttons["Not Now"].tap() }
        XCTAssertTrue(app.staticTexts["syntheticBanner"].waitForExistence(timeout: 20))
        XCTAssertTrue(app.tabBars.buttons["Settings"].waitForExistence(timeout: 30), "The preserved primary QA installation must reopen its protected store")

        app.tabBars.buttons["Settings"].tap()
        let fixture = app.staticTexts["qaFixtureStage"]
        XCTAssertTrue(fixture.waitForExistence(timeout: 10))
        // Current-record read supplies the actual baseline and revision without
        // starting another reconciliation generation.
        app.buttons["loadQANoteFixture"].tap()
        expectation(for: NSPredicate(format: "label == %@", "injection committed"), evaluatedWith: fixture)
        waitForExpectations(timeout: 30)

        let conflictActor = app.staticTexts["qaConflictStage"]
        XCTAssertTrue(conflictActor.waitForExistence(timeout: 10))
        app.buttons["qaFreshConflict"].tap()
        let fresh = XCTNSPredicateExpectation(predicate: NSPredicate(format: "label BEGINSWITH 'fresh note '"), object: conflictActor)
        XCTAssertEqual(XCTWaiter.wait(for: [fresh], timeout: 30), .completed, conflictActor.label)
        app.buttons["qaPrepareConflict"].tap()
        let prepared = XCTNSPredicateExpectation(predicate: NSPredicate(format: "label BEGINSWITH 'primary edit queued' OR label BEGINSWITH 'primary edit already queued'"), object: conflictActor)
        XCTAssertEqual(XCTWaiter.wait(for: [prepared], timeout: 10), .completed, conflictActor.label)
        app.buttons["qaAdvanceConflict"].tap()
        let advanced = XCTNSPredicateExpectation(predicate: NSPredicate(format: "label BEGINSWITH 'second actor accepted revision'"), object: conflictActor)
        XCTAssertEqual(XCTWaiter.wait(for: [advanced], timeout: 30), .completed, conflictActor.label)
        app.buttons["qaResumeSync"].tap()
        app.buttons["sync"].tap()
        app.tabBars.buttons["Saved work"].tap()
        let review = app.buttons["Review conflict"].firstMatch
        // Saved work retains earlier receipts; scroll the lazy List so the
        // fresh conflict row is realized rather than treating it as absent.
        for _ in 0..<6 where !review.exists { app.swipeUp() }
        XCTAssertTrue(review.waitForExistence(timeout: 15), "The stale primary operation must become a non-retrying explicit conflict")
        review.tap()
        XCTAssertTrue(app.staticTexts["Your saved edit"].waitForExistence(timeout: 10))
        XCTAssertTrue(app.staticTexts["Current version"].waitForExistence(timeout: 10))
        XCTAssertTrue(app.staticTexts.matching(NSPredicate(format: "label BEGINSWITH 'second actor current version'")).firstMatch.waitForExistence(timeout: 10))
        let reviewed = XCTAttachment(screenshot: app.screenshot()); reviewed.name = "mobile002-second-actor-conflict-review"; reviewed.lifetime = .keepAlways; add(reviewed)

        app.swipeUp()
        let revised = app.buttons["Prepare revised edit against current version"]
        XCTAssertTrue(revised.waitForExistence(timeout: 10)); revised.tap()
        XCTAssertTrue(app.staticTexts["draftStatus"].label.contains("Revised draft saved on device"))
        app.buttons["saveAction"].tap()
        app.tabBars.buttons["Settings"].tap()
        app.buttons["qaDrainConflict"].tap()
        let receiptStage = app.staticTexts["qaConflictStage"]
        let accepted = XCTNSPredicateExpectation(predicate: NSPredicate(format: "label BEGINSWITH 'target accepted receipt'"), object: receiptStage)
        XCTAssertEqual(XCTWaiter.wait(for: [accepted], timeout: 60), .completed, receiptStage.label)
        app.tabBars.buttons["Saved work"].tap()
        let receipt = XCTAttachment(screenshot: app.screenshot()); receipt.name = "mobile002-revised-edit-receipt"; receipt.lifetime = .keepAlways; add(receipt)
    }
    #endif

    #if MOBILE005_QA
    @MainActor func testMobile005NativeOfflineProfileTerminateRelaunchAndSynchronize() throws {
        continueAfterFailure = false
        let app = XCUIApplication(); app.launchArguments = ["--synthetic-keychain"]; app.launch()
        XCTAssertTrue(app.staticTexts["syntheticBanner"].waitForExistence(timeout: 20))
        if app.buttons["signIn"].exists {
            app.textFields["email"].tap(); app.textFields["email"].typeText("agent@mobile.test")
            app.secureTextFields["password"].tap(); app.secureTextFields["password"].typeText("Mobile-demo-only-123!")
            app.buttons["signIn"].tap()
        }
        XCTAssertTrue(app.tabBars.buttons["Settings"].waitForExistence(timeout: 90))
        app.tabBars.buttons["Settings"].tap(); setSwitch(app.switches["offlineToggle"], to: false); app.buttons["sync"].tap()
        XCTAssertTrue(app.staticTexts["100 people available offline"].waitForExistence(timeout: 240))
        expectation(for: NSPredicate(format: "label CONTAINS 'Synced. Complete downloaded workspace' OR label BEGINSWITH 'Download complete.'"), evaluatedWith: app.staticTexts["statusMessage"])
        waitForExpectations(timeout: 180)
        let profileCache = app.staticTexts["qaProfileConflictStage"]
        app.buttons["qaInspectProfileCache"].tap()
        XCTAssertTrue(profileCache.label.contains("qualified=yes editable=yes caps=yes"), profileCache.label)
        app.tabBars.buttons["Saved work"].tap()
        let queueCount = app.staticTexts["queueCount"]
        let initialPending = Int(queueCount.label.split(separator: " ").first ?? "-1") ?? -1
        XCTAssertGreaterThanOrEqual(initialPending, 0, queueCount.label)
        app.tabBars.buttons["People"].tap()
        let person = app.buttons["person_07cb08d0-56c3-43c1-a538-68573e5a24f0"]
        XCTAssertTrue(person.waitForExistence(timeout: 20)); person.tap()
        XCTAssertTrue(app.buttons["editProfile"].waitForExistence(timeout: 20))
        app.tabBars.buttons["Settings"].tap(); setSwitch(app.switches["offlineToggle"], to: true); app.tabBars.buttons["People"].tap()
        // Returning from Settings commonly restores the selected detail view;
        // only tap the list row when navigation actually returned to the list.
        if person.exists { person.tap() }
        app.buttons["editProfile"].tap()
        let name = app.textFields["profileFirstName"]; XCTAssertTrue(name.waitForExistence(timeout: 15)); name.tap(); name.typeText(" QA")
        let editedEmail = app.textFields["Email"].firstMatch; XCTAssertTrue(editedEmail.waitForExistence(timeout: 10)); editedEmail.tap()
        editedEmail.typeKey("a", modifierFlags: .command); editedEmail.typeText("ios.edited." + UUID().uuidString.lowercased() + "@example.test")
        let addEmail = app.buttons["profileAddEmail"]
        if !addEmail.exists { app.swipeDown() }
        XCTAssertTrue(addEmail.waitForExistence(timeout: 10)); addEmail.tap()
        let addedEmail = app.textFields["profileNewEmail"]
        if !addedEmail.exists { app.swipeUp() }
        XCTAssertTrue(addedEmail.waitForExistence(timeout: 10)); addedEmail.tap(); addedEmail.typeText("ios.added." + UUID().uuidString.lowercased() + "@example.test")
        XCTAssertTrue(app.staticTexts["profileDraftStatus"].waitForExistence(timeout: 10))
        let save = app.buttons["saveProfile"]
        if !save.exists { app.swipeDown() }
        XCTAssertTrue(save.waitForExistence(timeout: 10)); save.tap(); app.tabBars.buttons["Saved work"].tap()
        expectation(for: NSPredicate(format: "label BEGINSWITH %@", "\(initialPending + 1) pending"), evaluatedWith: queueCount); waitForExpectations(timeout: 20)
        app.tabBars.buttons["Settings"].tap(); let outbox = app.staticTexts["qaProfileConflictStage"]
        app.buttons["qaInspectProfileOutbox"].tap(); XCTAssertTrue(outbox.label.hasPrefix("profile outbox ")); let savedFingerprint = outbox.label
        let pending = XCTAttachment(screenshot: app.screenshot()); pending.name = "mobile005-profile-offline-pending"; pending.lifetime = .keepAlways; add(pending)
        app.terminate(); app.launch(); XCTAssertTrue(app.tabBars.buttons["Saved work"].waitForExistence(timeout: 30)); app.tabBars.buttons["Saved work"].tap()
        XCTAssertTrue(queueCount.label.hasPrefix("\(initialPending + 1) pending"), queueCount.label)
        app.tabBars.buttons["Settings"].tap(); app.buttons["qaInspectProfileOutbox"].tap(); XCTAssertEqual(outbox.label, savedFingerprint, "The exact immutable profile envelope must survive process termination.")
        setSwitch(app.switches["offlineToggle"], to: false); app.buttons["sync"].tap()
        var verified = false
        for _ in 0..<30 where !verified {
            app.buttons["qaVerifyProfileReceipt"].tap()
            let expectation = XCTNSPredicateExpectation(predicate: NSPredicate(format: "label BEGINSWITH 'profile receipt verified'"), object: outbox)
            verified = XCTWaiter.wait(for: [expectation], timeout: 2) == .completed
        }
        XCTAssertTrue(verified, outbox.label)
        let accepted = XCTAttachment(screenshot: app.screenshot()); accepted.name = "mobile005-profile-synced"; accepted.lifetime = .keepAlways; add(accepted)
        app.tabBars.buttons["Settings"].tap(); setSwitch(app.switches["offlineToggle"], to: true)
    }
    @MainActor func testMobile005NativeProfileConflictReviewCurrentAndManualReplacement() throws {
        continueAfterFailure = false
        let app = XCUIApplication(); app.launchArguments = ["--synthetic-keychain"]; app.launch()
        XCTAssertTrue(app.staticTexts["syntheticBanner"].waitForExistence(timeout: 20))
        if app.buttons["signIn"].exists {
            app.textFields["email"].tap(); app.textFields["email"].typeText("agent@mobile.test")
            app.secureTextFields["password"].tap(); app.secureTextFields["password"].typeText("Mobile-demo-only-123!")
            app.buttons["signIn"].tap()
        }
        XCTAssertTrue(app.tabBars.buttons["Settings"].waitForExistence(timeout: 90)); app.tabBars.buttons["Settings"].tap()
        setSwitch(app.switches["offlineToggle"], to: true)
        let stage = app.staticTexts["qaProfileConflictStage"]
        XCTAssertTrue(stage.waitForExistence(timeout: 15)); app.buttons["qaPrepareProfileConflict"].tap()
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "label BEGINSWITH 'primary profile queued'"), object: stage)], timeout: 20), .completed, stage.label)
        setSwitch(app.switches["offlineToggle"], to: false); app.buttons["qaAdvanceProfileConflict"].tap()
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "label BEGINSWITH 'second actor accepted profile'"), object: stage)], timeout: 45), .completed, stage.label)
        app.buttons["qaDrainProfileConflict"].tap()
        app.tabBars.buttons["Saved work"].tap(); let review = app.buttons["Review conflict"].firstMatch
        for _ in 0..<6 where !review.exists { app.swipeUp() }
        XCTAssertTrue(review.waitForExistence(timeout: 30)); review.tap()
        XCTAssertTrue(app.staticTexts["Your saved proposal"].waitForExistence(timeout: 15))
        XCTAssertTrue(app.staticTexts["Current profile"].waitForExistence(timeout: 15), "Replacement remains unavailable until the complete current profile has been read.")
        let reviewed = XCTAttachment(screenshot: app.screenshot()); reviewed.name = "mobile005-profile-conflict-current"; reviewed.lifetime = .keepAlways; add(reviewed)
        let replacement = app.buttons["prepareProfileReplacement"]
        XCTAssertTrue(replacement.waitForExistence(timeout: 15)); replacement.tap()
        let replacementStatus = app.staticTexts["profileDraftStatus"]
        XCTAssertTrue(replacementStatus.waitForExistence(timeout: 10)); XCTAssertTrue(replacementStatus.label.contains("Replacement draft saved"))
        let save = app.buttons["saveProfile"]
        XCTAssertTrue(save.waitForExistence(timeout: 10)); save.tap(); app.tabBars.buttons["Settings"].tap(); app.buttons["qaDrainProfileConflict"].tap()
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "label BEGINSWITH 'profile accepted'"), object: stage)], timeout: 60), .completed, stage.label)
        app.tabBars.buttons["Saved work"].tap(); let receipt = XCTAttachment(screenshot: app.screenshot()); receipt.name = "mobile005-profile-replacement-accepted"; receipt.lifetime = .keepAlways; add(receipt)
        app.tabBars.buttons["Settings"].tap(); setSwitch(app.switches["offlineToggle"], to: true)
    }
    #endif

}
