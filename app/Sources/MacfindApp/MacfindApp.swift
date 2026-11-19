import AppKit
import SwiftUI

final class MacfindAppDelegate: NSObject, NSApplicationDelegate {
    func applicationDidFinishLaunching(_ notification: Notification) {
        // Nécessaire quand l'exécutable est lancé hors bundle (`swift run`).
        NSApp.setActivationPolicy(.regular)
        NSApp.activate(ignoringOtherApps: true)
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        true
    }
}

@main
struct MacfindApp: App {
    @NSApplicationDelegateAdaptor(MacfindAppDelegate.self) private var delegue

    var body: some Scene {
        Window("macfind", id: "principale") {
            ContentView()
                .frame(minWidth: 860, minHeight: 480)
        }
        .defaultSize(width: 1180, height: 680)
    }
}
