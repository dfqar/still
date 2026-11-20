import SwiftUI
import AppKit

@main
struct StillApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var appDelegate
    @StateObject private var model = AppModel()

    var body: some Scene {
        WindowGroup {
            ContentView(model: model)
                .frame(minWidth: 640, minHeight: 420)
                .background(Color.black)
                .preferredColorScheme(.dark)
                .onAppear {
                    appDelegate.model = model
                    model.prepare()
                }
        }
        .defaultSize(width: 1100, height: 720)
        .windowStyle(.hiddenTitleBar)
        .commands { StillCommands(model: model) }
    }
}

@MainActor final class AppDelegate: NSObject, NSApplicationDelegate {
    weak var model: AppModel?
    private var tokens: [NSObjectProtocol] = []

    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.appearance = NSAppearance(named: .darkAqua)
        NSApp.setActivationPolicy(.regular)
        observeWindows()
        DispatchQueue.main.async {
            if !AppModel.diagnosticRun { NSApp.windows.first?.setFrameAutosaveName("Still.MainWindow") }
            NSApp.windows.first?.titleVisibility = .hidden
            NSApp.windows.first?.titlebarAppearsTransparent = true
            if !AppModel.diagnosticRun && UserDefaults.standard.bool(forKey: "window.wasFullscreen") {
                NSApp.windows.first?.toggleFullScreen(nil)
            }
        }
    }

    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        guard let model else { return .terminateNow }
        Task {
            await model.shutdown()
            sender.reply(toApplicationShouldTerminate: true)
        }
        return .terminateLater
    }

    func applicationWillTerminate(_ notification: Notification) { NSCursor.unhide() }

    private func observeWindows() {
        let center = NotificationCenter.default
        tokens.append(center.addObserver(forName: NSWindow.didEnterFullScreenNotification, object: nil, queue: .main) { _ in
            if !AppModel.diagnosticRun { UserDefaults.standard.set(true, forKey: "window.wasFullscreen") }
        })
        tokens.append(center.addObserver(forName: NSWindow.didExitFullScreenNotification, object: nil, queue: .main) { _ in
            if !AppModel.diagnosticRun { UserDefaults.standard.set(false, forKey: "window.wasFullscreen") }
        })
    }
}

struct StillCommands: Commands {
    @ObservedObject var model: AppModel
    var body: some Commands {
        CommandGroup(replacing: .appInfo) {
            Button("About Still") {
                NSApp.orderFrontStandardAboutPanel(options: [
                    .applicationName: "Still",
                    .applicationVersion: "2.2",
                    .version: "5",
                    .credits: NSAttributedString(string: "Six generative ambient spaces rendered in real time with Rust, wgpu and Metal.")
                ])
            }
        }
        CommandGroup(replacing: .newItem) {
            Button("New Variation") { model.newVariation() }
                .keyboardShortcut("n", modifiers: .command)
                .disabled(model.engine == nil)
        }
        CommandGroup(after: .toolbar) {
            Button("Show Controls") { model.noteActivity() }
                .keyboardShortcut(",", modifiers: .command)
            Divider()
            Button("Enter Full Screen") {
                model.noteActivity(); NSApp.keyWindow?.toggleFullScreen(nil)
            }
            .keyboardShortcut("f", modifiers: .command)
        }
        CommandGroup(after: .saveItem) {
            Button("Save Current Preset…") { model.showingSavePreset = true; model.noteActivity() }
                .keyboardShortcut("s", modifiers: .command)
                .disabled(model.engine == nil)
        }
        CommandMenu("Spaces") {
            ForEach(Array(model.scenes.enumerated()), id: \.element.id) { index, scene in
                Button((model.selectedScene == scene.id ? "✓ " : "") + scene.name) { model.selectScene(scene.id) }
                    .keyboardShortcut(KeyEquivalent(Character(String(index + 1))), modifiers: .command)
            }
        }
        CommandMenu("Quality") {
            qualityItem("Auto", value: 0)
            qualityItem("Eco", value: 1)
            qualityItem("Balanced", value: 2)
            qualityItem("High", value: 3)
        }
        CommandMenu("Frame Rate") {
            frameRateItem("Auto", value: 0)
            frameRateItem("30 Hz", value: 30)
            frameRateItem("60 Hz", value: 60)
            frameRateItem("Up to 120 Hz", value: 120)
        }
        CommandMenu("Power") {
            powerItem("Automatic", value: 0)
            powerItem("Quality First", value: 1)
            powerItem("Efficiency", value: 2)
        }
    }

    private func qualityItem(_ title: String, value: UInt32) -> some View {
        Button((model.quality == value ? "✓ " : "") + title) { model.setQuality(value) }
    }
    private func frameRateItem(_ title: String, value: Int) -> some View {
        Button((model.frameRatePreference == value ? "✓ " : "") + title) { model.setFrameRatePreference(value) }
    }
    private func powerItem(_ title: String, value: Int) -> some View {
        Button((model.powerPolicy == value ? "✓ " : "") + title) { model.setPowerPolicy(value) }
    }
}
