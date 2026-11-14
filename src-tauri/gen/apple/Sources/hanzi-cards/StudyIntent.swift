// Siri and the Shortcuts app: "Start studying in Hanzi Cards".
//
// The intent opens the app and leaves a one-word request in the app's data folder (the folder
// Tauri's app_data_dir points at); the page picks it up when the app comes to the front
// (commands::pending_action) and starts a session with the learner's usual choices.

import AppIntents
import Foundation

@available(iOS 16.0, *)
struct StartStudyingIntent: AppIntent {
    static let title: LocalizedStringResource = "Start studying"
    static let description = IntentDescription("Opens Hanzi Cards and starts a study session with your usual choices.")
    static let openAppWhenRun: Bool = true

    @MainActor
    func perform() async throws -> some IntentResult {
        let dir = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("com.hanzicards.app", isDirectory: true)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        try? "study".write(to: dir.appendingPathComponent("siri-action"), atomically: true, encoding: .utf8)
        return .result()
    }
}

@available(iOS 16.0, *)
struct HanziCardsShortcuts: AppShortcutsProvider {
    static var appShortcuts: [AppShortcut] {
        AppShortcut(
            intent: StartStudyingIntent(),
            phrases: [
                "Start studying in \(.applicationName)",
                "Study Chinese with \(.applicationName)",
                "Study my cards in \(.applicationName)",
            ],
            shortTitle: "Start studying",
            systemImageName: "rectangle.stack"
        )
    }
}
