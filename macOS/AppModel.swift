import SwiftUI
import AppKit

struct ParameterPresentation: Identifiable {
    let id: String
    let name: String
    let range: ClosedRange<Double>
    let defaultValue: Double
}

struct StillPreparationFailure: Error, Sendable {
    let message: String
}

@MainActor
final class AppModel: ObservableObject {
    typealias EngineFactory = @Sendable (UInt32) -> Result<EngineController, StillPreparationFailure>
    @Published private(set) var engine: EngineController?
    @Published private(set) var isPreparing = false
    @Published var descriptors: [ParameterPresentation] = []
    @Published var values: [String: Double] = [:]
    @Published var controlsVisible = true
    @Published var quality: UInt32
    @Published var frameRatePreference: Int
    @Published var powerPolicy: Int
    @Published var presets: [PresetPresentation] = []
    @Published var selectedPreset: String?
    @Published var scenes: [ScenePresentation] = []
    @Published var selectedScene: String?
    @Published var showingSavePreset = false
    @Published private(set) var renderingError: String?
    private let engineFactory: EngineFactory
    private var preparation: Task<Void, Never>?
    private var retiringEngine: Task<Void, Never>?
    private var generation: UInt64 = 0
    private var stopped = false
    private var recoveredState: Data?
    private var idleTimer: Timer?
    private var cursorHidden = false
    private var saveTask: DispatchWorkItem?
    private var activityMonitor: Any?
    private var stateWritesBlocked = false
    private var startedDiagnostics = false
    private var diagnosticCycleTimer: Timer?
    private var diagnosticMetricsTimer: Timer?
    private var diagnosticExitTimer: Timer?
    nonisolated static let diagnosticRun = ProcessInfo.processInfo.environment["STILL_PROFILE_RUN"] == "1"
        || ProcessInfo.processInfo.environment["STILL_INITIAL_SCENE"] != nil
    private let diagnosticSceneOverride = ProcessInfo.processInfo.environment["STILL_INITIAL_SCENE"]

    init(engineFactory: EngineFactory? = nil) {
        let environment = ProcessInfo.processInfo.environment
        let rawQuality = Self.diagnosticRun ? Int(environment["STILL_QUALITY"] ?? "2") ?? 2 : UserDefaults.standard.integer(forKey: "device.quality")
        let savedQuality = UInt32((0...3).contains(rawQuality) ? rawQuality : 0)
        let rawFrameRate = Self.diagnosticRun ? Int(environment["STILL_FRAME_RATE"] ?? "60") ?? 60 : UserDefaults.standard.integer(forKey: "device.frameRate")
        let savedFrameRate = [0, 30, 60, 120].contains(rawFrameRate) ? rawFrameRate : 0
        let savedPowerPolicy = UserDefaults.standard.integer(forKey: "device.powerPolicy")
        quality = savedQuality
        frameRatePreference = savedFrameRate
        powerPolicy = Self.diagnosticRun ? 1 : ((0...2).contains(savedPowerPolicy) ? savedPowerPolicy : 0)
        self.engineFactory = engineFactory ?? { quality in
            let controller = EngineController(quality: quality)
            if let message = controller.initializationError {
                return .failure(StillPreparationFailure(message: message))
            }
            return .success(controller)
        }
        if Self.diagnosticRun {
            diagnosticMetricsTimer = Timer.scheduledTimer(withTimeInterval: 10, repeats: true) { [weak self] _ in
                MainActor.assumeIsolated { self?.writeDiagnosticMetricsIfRequested() }
            }
            if let seconds = environment["STILL_EXIT_AFTER_SECONDS"].flatMap(TimeInterval.init), seconds > 0 {
                diagnosticExitTimer = Timer.scheduledTimer(withTimeInterval: seconds, repeats: false) { _ in
                    MainActor.assumeIsolated { NSApp.terminate(nil) }
                }
            }
        }
        noteActivity()
        activityMonitor = NSEvent.addLocalMonitorForEvents(matching: [.keyDown, .leftMouseDown, .rightMouseDown, .scrollWheel]) { [weak self] event in
            MainActor.assumeIsolated { self?.noteActivity() }
            return event
        }
    }

    deinit {
        preparation?.cancel()
        if cursorHidden { NSCursor.unhide() }
        if let activityMonitor { NSEvent.removeMonitor(activityMonitor) }
        diagnosticCycleTimer?.invalidate()
        diagnosticMetricsTimer?.invalidate()
        diagnosticExitTimer?.invalidate()
    }

    func prepare() {
        guard !stopped, engine == nil, !isPreparing, renderingError == nil else { return }
        generation &+= 1
        let token = generation
        let factory = engineFactory
        let quality = self.quality
        let retiringEngine = self.retiringEngine
        isPreparing = true
        preparation = Task { [weak self] in
            await retiringEngine?.value
            guard !Task.isCancelled else { return }
            let result = await Task.detached(priority: .userInitiated) { factory(quality) }.value
            guard let self, generation == token, !Task.isCancelled else {
                if case let .success(controller) = result { await controller.close() }
                return
            }
            preparation = nil
            self.retiringEngine = nil
            isPreparing = false
            switch result {
            case let .failure(error): renderingError = error.message
            case let .success(controller): installEngine(controller, generation: token)
            }
        }
    }

    func retry() {
        guard !stopped, renderingError != nil else { return }
        renderingError = nil
        prepare()
    }

    func shutdown() async {
        guard !stopped else { return }
        stopped = true
        generation &+= 1
        let pending = preparation
        pending?.cancel()
        preparation = nil
        isPreparing = false
        saveTask?.cancel()
        saveNow()
        writeDiagnosticMetricsIfRequested()
        let controller = engine
        engine = nil
        if let controller { await controller.close() }
        await retiringEngine?.value
        retiringEngine = nil
        await pending?.value
    }

    private func installEngine(_ controller: EngineController, generation token: UInt64) {
        controller.onError = { [weak self] message in
            Task { @MainActor in self?.engineFailed(message, generation: token) }
        }
        let restored = recoveredState.map(controller.importState) ?? false
        recoveredState = nil
        if !restored {
            if !Self.diagnosticRun { loadState(into: controller) }
            if let diagnosticSceneOverride, !diagnosticSceneOverride.isEmpty {
                _ = controller.selectScene(diagnosticSceneOverride)
            }
            if let preset = ProcessInfo.processInfo.environment["STILL_INITIAL_PRESET"] {
                _ = controller.selectPreset(preset)
            }
        }
        controller.setQuality(quality)
        engine = controller
        refreshSceneContext()
        noteActivity()
        if !startedDiagnostics {
            startedDiagnostics = true
            startDiagnosticSceneCycleIfRequested()
            startLifecycleDiagnosticIfRequested()
        }
    }

    private func engineFailed(_ message: String, generation token: UInt64) {
        guard generation == token, let controller = engine else { return }
        writeDiagnosticMetricsIfRequested()
        recoveredState = controller.exportState()
        renderingError = message
        engine = nil
        retiringEngine = Task { await controller.close() }
    }

    func set(_ id: String, to value: Double) {
        guard let engine else { return }
        let descriptor = descriptors.first(where: { $0.id == id })
        let validated = min(max(value, descriptor?.range.lowerBound ?? value), descriptor?.range.upperBound ?? value)
        values[id] = validated
        engine.setParameter(id, value: Float(validated))
        selectedPreset = nil
        scheduleSave()
        noteActivity()
    }

    func newVariation() {
        guard let engine else { return }
        selectedPreset = nil; engine.newVariation(); scheduleSave(); noteActivity()
    }

    func setColor(prefix: String, red: Double, green: Double, blue: Double) {
        guard let engine else { return }
        // One engine queue operation publishes RGB together between frames.
        let components = [(prefix + "red", red), (prefix + "green", green), (prefix + "blue", blue)]
        for (id, value) in components { values[id] = min(max(value, 0), 1) }
        engine.setParameters(components.map { ($0.0, Float($0.1)) })
        selectedPreset = nil
        scheduleSave()
        noteActivity()
    }

    func setQuality(_ value: UInt32) {
        quality = value
        if !Self.diagnosticRun { UserDefaults.standard.set(Int(value), forKey: "device.quality") }
        engine?.setQuality(value)
        noteActivity()
    }

    func setFrameRatePreference(_ value: Int) {
        frameRatePreference = value
        if !Self.diagnosticRun { UserDefaults.standard.set(value, forKey: "device.frameRate") }
        noteActivity()
    }

    func setPowerPolicy(_ value: Int) {
        powerPolicy = min(max(value, 0), 2)
        if !Self.diagnosticRun { UserDefaults.standard.set(powerPolicy, forKey: "device.powerPolicy") }
        noteActivity()
    }

    func selectPreset(_ id: String) {
        guard engine?.selectPreset(id) == true else { return }
        refreshValues(); refreshPresets(); scheduleSave(); noteActivity()
    }

    func selectScene(_ id: String) {
        guard id != selectedScene, engine?.selectScene(id) == true else { return }
        NSColorPanel.shared.close()
        refreshSceneContext()
        scheduleSave()
        noteActivity()
    }

    func savePreset(named name: String) -> Bool {
        guard engine?.saveUserPreset(named: name) == true else { return false }
        refreshPresets(); scheduleSave(); noteActivity(); return true
    }

    func deleteSelectedUserPreset() {
        guard let id = selectedPreset, id.hasPrefix("user."), let preset = presets.first(where: { $0.id == id }) else { return }
        guard engine?.deleteUserPreset(named: preset.name) == true else { return }
        refreshPresets(); scheduleSave(); noteActivity()
    }

    func noteActivity() {
        controlsVisible = true
        if cursorHidden { NSCursor.unhide(); cursorHidden = false }
        idleTimer?.invalidate()
        idleTimer = Timer.scheduledTimer(withTimeInterval: 2.5, repeats: false) { [weak self] _ in
            Task { @MainActor in
                guard let self else { return }
                guard self.engine != nil, !self.showingSavePreset && !NSColorPanel.shared.isVisible else { return }
                self.controlsVisible = false
                if NSApp.isActive && NSApp.keyWindow != nil { NSCursor.hide(); self.cursorHidden = true }
            }
        }
    }

    func applicationBecameInactive() {
        idleTimer?.invalidate()
        if cursorHidden { NSCursor.unhide(); cursorHidden = false }
    }

    private func refreshValues() {
        values.removeAll(keepingCapacity: true)
        for d in descriptors { values[d.id] = Double(engine?.parameter(d.id) ?? Float(d.defaultValue)) }
    }

    private func refreshPresets() {
        guard let catalog = engine?.presetCatalog() else { return }
        presets = catalog.presets; selectedPreset = catalog.selected
    }

    private func refreshSceneContext() {
        guard let engine else { return }
        let catalog = engine.sceneCatalog()
        scenes = catalog.scenes
        selectedScene = catalog.selected
        descriptors = engine.parameterDescriptors()
        refreshValues()
        refreshPresets()
    }

    private func startDiagnosticSceneCycleIfRequested() {
        guard let raw = ProcessInfo.processInfo.environment["STILL_SCENE_CYCLE_SECONDS"],
              let interval = TimeInterval(raw), interval >= 1 else { return }
        diagnosticCycleTimer = Timer.scheduledTimer(withTimeInterval: interval, repeats: true) { [weak self] _ in
            Task { @MainActor in
                guard let self, !self.scenes.isEmpty else { return }
                let current = self.scenes.firstIndex(where: { $0.id == self.selectedScene }) ?? 0
                self.selectScene(self.scenes[(current + 1) % self.scenes.count].id)
            }
        }
    }

    // Explicit opt-in test of the real native window and notification paths.
    // This never puts the computer to sleep or changes persisted preferences.
    private func startLifecycleDiagnosticIfRequested() {
        guard Self.diagnosticRun, let path = ProcessInfo.processInfo.environment["STILL_LIFECYCLE_PATH"] else { return }
        var evidence: [[String: Any]] = []
        var before: UInt64 = 0
        var diagnosticWindow: NSWindow?
        func testWindow() -> NSWindow? {
            // Keep testing the canvas window even if AppKit reorders its window
            // list when the color panel opens, the canvas minimizes, or Spaces changes.
            if diagnosticWindow == nil {
                diagnosticWindow = NSApp.windows.first { window in
                    window.contentView.flatMap(metalHost(in:)) != nil
                }
            }
            return diagnosticWindow
        }
        func frames() -> UInt64 {
            guard let data = engine?.exportMetrics(), let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { return 0 }
            return (object["presented_frames"] as? NSNumber)?.uint64Value ?? 0
        }
        func record(_ action: String, _ pass: Bool) {
            let window = testWindow()
            evidence.append(["action": action, "passed": pass, "frames": frames(),
                             "app_active": NSApp.isActive, "window_visible": window?.isVisible ?? false,
                             "window_miniaturized": window?.isMiniaturized ?? false,
                             "window_occluded": window?.occlusionState.contains(.visible) != true,
                             "rendering_error": renderingError ?? ""])
            if let data = try? JSONSerialization.data(withJSONObject: evidence, options: [.prettyPrinted, .sortedKeys]) {
                try? data.write(to: URL(fileURLWithPath: path), options: .atomic)
            }
        }
        func later(_ seconds: Double, _ action: @escaping @MainActor () -> Void) {
            DispatchQueue.main.asyncAfter(deadline: .now() + seconds) { MainActor.assumeIsolated { action() } }
        }
        NSApp.activate(ignoringOtherApps: true)
        let spaceIDs = scenes.map(\.id)
        guard !spaceIDs.isEmpty else { record("scene-catalog", false); return }
        for (index, space) in spaceIDs.enumerated() {
            let base = Double(index * 32 + 2)
            later(base) { [self] in selectScene(space); record("\(space):select", selectedScene == space) }
            later(base + 2) {
                testWindow()?.setContentSize(NSSize(width: 800, height: 520))
                record("\(space):resize-small", testWindow() != nil)
            }
            later(base + 4) { testWindow()?.setContentSize(NSSize(width: 1180, height: 760)) }
            later(base + 6) { testWindow()?.toggleFullScreen(nil) }
            later(base + 9) { record("\(space):fullscreen", testWindow()?.styleMask.contains(.fullScreen) == true) }
            later(base + 10) { testWindow()?.toggleFullScreen(nil) }
            later(base + 13) { record("\(space):windowed", testWindow()?.styleMask.contains(.fullScreen) == false) }
            later(base + 14) { before = frames(); testWindow()?.miniaturize(nil) }
            later(base + 16) {
                record("\(space):minimized-suspends", testWindow()?.isMiniaturized == true && frames() <= before + 2)
                before = frames()
            }
            later(base + 17) {
                let window = testWindow()
                window?.deminiaturize(nil)
                window?.makeKeyAndOrderFront(nil)
                NSApp.activate(ignoringOtherApps: true)
            }
            later(base + 23) { record("\(space):restore-renders", testWindow()?.isMiniaturized == false && frames() > before + 20) }
            later(base + 24) {
                NSWorkspace.shared.notificationCenter.post(name: NSWorkspace.willSleepNotification, object: nil)
                before = frames()
            }
            later(base + 25) { record("\(space):sleep-notification-suspends", frames() <= before + 2); before = frames() }
            later(base + 26) {
                NSWorkspace.shared.notificationCenter.post(name: NSWorkspace.didWakeNotification, object: nil)
                NSApp.activate(ignoringOtherApps: true)
                testWindow()?.makeKeyAndOrderFront(nil)
            }
            later(base + 28) { record("\(space):wake-notification-renders", frames() > before + 20) }
        }
        let rapidStart = Double(spaceIDs.count * 32 + 2)
        later(rapidStart - 0.5) { NSApp.activate(ignoringOtherApps: true); testWindow()?.makeKeyAndOrderFront(nil) }
        for index in 0..<30 {
            later(rapidStart + Double(index) * 0.08) { [self] in selectScene(spaceIDs[index % spaceIDs.count]) }
        }
        later(rapidStart + 4) { record("rapid-switches", frames() > before + 100) }
        later(rapidStart + 5) { [self] in
            let previous = selectedScene
            let current = spaceIDs.firstIndex(where: { $0 == previous }) ?? 0
            selectScene(spaceIDs[(current + 1) % spaceIDs.count])
            let window = testWindow()
            window?.close()
            record("close-during-crossfade", previous != selectedScene && window?.isVisible == false && renderingError == nil)
        }
    }

    private func stateURL() -> URL? {
        FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first?
            .appendingPathComponent("Still", isDirectory: true).appendingPathComponent("state-v3.json")
    }

    private func loadState(into engine: EngineController) {
        guard let url = stateURL() else { return }
        let directory = url.deletingLastPathComponent()
        let candidates = [
            url,
            directory.appendingPathComponent("state-v2.json"),
            directory.appendingPathComponent("state-v1.json"),
        ]
        for source in candidates {
            let data: Data
            do {
                data = try Data(contentsOf: source)
            } catch {
                let failure = error as NSError
                if failure.domain == NSCocoaErrorDomain && failure.code == NSFileReadNoSuchFileError {
                    continue
                }
                if source == url { stateWritesBlocked = true }
                NSLog("Still could not read state at \(source.path): \(error.localizedDescription)")
                return
            }
            if !engine.importState(data) {
                let preserved = preserveRejectedState(at: source)
                if source == url && !preserved { stateWritesBlocked = true }
            }
            return
        }
    }

    private func preserveRejectedState(at url: URL) -> Bool {
        let backup = url.appendingPathExtension("rejected-\(UUID().uuidString)")
        do {
            try FileManager.default.copyItem(at: url, to: backup)
            NSLog("Still preserved rejected state at \(backup.path)")
            return true
        } catch {
            NSLog("Still could not preserve rejected state: \(error.localizedDescription)")
            return false
        }
    }

    private func scheduleSave() {
        guard !Self.diagnosticRun else { return }
        saveTask?.cancel()
        let task = DispatchWorkItem { [weak self] in Task { @MainActor in self?.saveNow() } }
        saveTask = task
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.35, execute: task)
    }

    private func saveNow() {
        guard !Self.diagnosticRun, !stateWritesBlocked else { return }
        guard let url = stateURL(), let data = engine?.exportState() ?? recoveredState else { return }
        do {
            try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
            try data.write(to: url, options: .atomic)
        } catch {
            NSLog("Still state save failed: \(error.localizedDescription)")
        }
    }

    private func writeDiagnosticMetricsIfRequested() {
        guard let path = ProcessInfo.processInfo.environment["STILL_METRICS_PATH"],
              !path.isEmpty,
              let raw = engine?.exportMetrics(),
              var metrics = try? JSONSerialization.jsonObject(with: raw) as? [String: Any] else { return }
        for window in NSApp.windows {
            if let view = window.contentView, let bytes = metalHost(in: view)?.allocatedMetalBytes {
                metrics["metal_allocated_bytes"] = bytes
                break
            }
        }
        guard let data = try? JSONSerialization.data(withJSONObject: metrics, options: [.prettyPrinted, .sortedKeys]) else { return }
        do {
            try data.write(to: URL(fileURLWithPath: path), options: .atomic)
        } catch {
            NSLog("Still metrics save failed: \(error.localizedDescription)")
        }
    }

    private func metalHost(in view: NSView) -> MetalHostView? {
        if let host = view as? MetalHostView { return host }
        for child in view.subviews {
            if let host = metalHost(in: child) { return host }
        }
        return nil
    }
}
