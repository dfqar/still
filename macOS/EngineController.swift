import Foundation
import AppKit

/// Visual controls only; an adapter decides how sound or other data maps here.
struct AmbientLightInput {
    var color: (Float, Float, Float)
    var gain: Float = 1
    var offset: (Float, Float) = (0, 0)
    var scale: (Float, Float) = (1, 1)
    var rotationDegrees: Float = 0

    var isFinite: Bool {
        color.0.isFinite && color.1.isFinite && color.2.isFinite && gain.isFinite
            && offset.0.isFinite && offset.1.isFinite && scale.0.isFinite && scale.1.isFinite && rotationDegrees.isFinite
    }
    var ffi: StillAmbientLightControl {
        StillAmbientLightControl(color: color, gain: gain, offset: offset, scale: scale, rotation_degrees: rotationDegrees)
    }
}

struct PresetPresentation: Codable, Identifiable, Equatable {
    let id: String
    let name: String
    let builtin: Bool
}

struct ScenePresentation: Codable, Identifiable, Equatable {
    let id: String
    let name: String
    let schemaVersion: UInt32
    let supportsModulation: Bool
}

/// Generic visual overlay with a caller-defined time base.
struct StillModulationInput: Sendable, Equatable {
    let timeSeconds: Double
    let motion, turbulence, light: Float
    var isValid: Bool { timeSeconds.isFinite && timeSeconds >= 0 && [motion, turbulence, light].allSatisfy { $0.isFinite && (0...1).contains($0) } }
    var ffi: StillModulationFrame { StillModulationFrame(abi_version: still_abi_version(), struct_size: UInt32(MemoryLayout<StillModulationFrame>.size), time_seconds: timeSeconds, motion: motion, turbulence: turbulence, light: light, reserved: 0) }
}

private struct SceneCatalog: Codable {
    let selected: String
    let scenes: [ScenePresentation]
}

private struct ParameterCatalog: Codable {
    struct Parameter: Codable {
        let id: String
        let name: String
        let minimum: Double
        let maximum: Double
        let defaultValue: Double
    }

    let scene: String
    let parameters: [Parameter]
}

private struct PresetCatalog: Codable {
    let selected: String?
    let presets: [PresetPresentation]
}

/// Lives until the last admitted render-queue operation finishes. Never closes on a frame callback.
private final class StillHandleOwner: @unchecked Sendable {
    var handle: OpaquePointer?
    var suspended = false
    var attached = false
    var surfaceGeneration: UInt64 = 0
    var frameFailures = 0
    var reportedFailure = false
    var modulationDeadline: CFTimeInterval = 0
    var hasModulation = false
    func close() {
        attached = false
        if let handle { _ = still_engine_surface_detach(handle); still_engine_destroy(handle); self.handle = nil }
    }
    deinit { close() }
}

final class EngineController: @unchecked Sendable {
    private let queue = DispatchQueue(label: "app.still.engine", qos: .userInteractive)
    private let pendingLock = NSLock()
    private var errorHandler: (@Sendable (String) -> Void)?
    private var latestFrameTimestamp: CFTimeInterval = 0
    private var activeSurface: UInt64?
    private var latestFrameSurface: UInt64?
    private var latestFrameAnimated = false
    private var pendingAmbientFrame: StillAmbientFrame?
    private var hasPendingAmbientUpdate = false
    private var pendingAmbientControls: StillAmbientControlFrame?
    private var hasPendingControlsUpdate = false
    private var pendingAmbientSmoothing: StillAmbientSmoothing?
    private var hasPendingSmoothingUpdate = false
    private var pendingModulation: StillModulationFrame?
    private var pendingModulationDeadline: CFTimeInterval = 0
    private var hasPendingModulationUpdate = false
    private let owner = StillHandleOwner()
    private var handle: OpaquePointer? { owner.handle }
    let initializationError: String?
    var onError: (@Sendable (String) -> Void)? {
        get { pendingLock.withLock { errorHandler } }
        set { pendingLock.withLock { errorHandler = newValue } }
    }
    private lazy var frameSource: DispatchSourceUserDataAdd = {
        let source = DispatchSource.makeUserDataAddSource(queue: queue)
        source.setEventHandler { [weak self] in self?.drainLatestFrame() }
        source.resume()
        return source
    }()

    init(quality: UInt32) {
        var config = StillEngineConfig(abi_version: still_abi_version(), struct_size: UInt32(MemoryLayout<StillEngineConfig>.size), quality: quality, reserved: 0)
        var created: OpaquePointer?
        let result = still_engine_create(&config, &created)
        if result == 0, let created {
            owner.handle = created
            initializationError = nil
        } else {
            initializationError = "The graphics device is unavailable."
            NSLog("Still renderer initialization failed (status \(result))")
        }
        _ = frameSource
    }

    deinit {
        frameSource.cancel()
        let owner = self.owner
        // The final weak work item may release this controller on its own render queue.
        queue.async { owner.close() }
    }

    @MainActor func close() async {
        pendingLock.withLock { activeSurface = nil; latestFrameSurface = nil }
        frameSource.cancel()
        let owner = self.owner
        await withCheckedContinuation { continuation in
            queue.async {
                owner.close()
                continuation.resume()
            }
        }
    }

    @discardableResult
    @MainActor func attach(view: NSView, physicalSize: CGSize) -> UInt64? {
        precondition(Thread.isMainThread, "CAMetalLayer surfaces must be attached on the AppKit thread")
        // Quarantine all queued FFI work for the entire exceptional AppKit-only attach.
        // Merely draining the queue first would allow a new producer to race this call.
        queue.sync { queue.suspend() }
        defer { queue.resume() }
        guard let handle else { return nil }
        let pointer = Unmanaged.passUnretained(view).toOpaque()
        let result = still_engine_surface_attach(handle, pointer, UInt32(max(0, physicalSize.width)), UInt32(max(0, physicalSize.height)))
        owner.surfaceGeneration &+= 1
        owner.attached = result == 0
        pendingLock.lock(); activeSurface = result == 0 ? owner.surfaceGeneration : nil; latestFrameSurface = nil; pendingLock.unlock()
        owner.frameFailures = 0; owner.reportedFailure = false
        logIfNeeded(result, operation: "surface attach")
        return result == 0 ? owner.surfaceGeneration : nil
    }

    /// Synchronously detaches; the caller must keep the NSView/layer alive until this returns.
    func detach(surface: UInt64) {
        queue.sync {
            guard owner.attached, owner.surfaceGeneration == surface else { return }
            pendingLock.lock(); activeSurface = nil; latestFrameSurface = nil; pendingLock.unlock()
            owner.attached = false
            if let handle { self.logIfNeeded(still_engine_surface_detach(handle), operation: "surface detach") }
        }
    }

    func resize(_ physicalSize: CGSize, surface: UInt64) {
        queue.async { [weak self] in
            guard let self, let handle, owner.attached, owner.surfaceGeneration == surface else { return }
            _ = still_engine_resize(handle, UInt32(max(0, physicalSize.width)), UInt32(max(0, physicalSize.height)))
        }
    }

    func setSuspended(_ suspended: Bool, surface: UInt64) {
        queue.async { [weak self] in
            guard let self, let handle, owner.attached, owner.surfaceGeneration == surface else { return }
            owner.suspended = suspended
            _ = suspended ? still_engine_suspend(handle) : still_engine_resume(handle)
        }
    }

    /// A bounded first paint for a visible, inactive window. It does not start a render loop.
    func renderStillFrame(_ timestamp: CFTimeInterval, surface: UInt64) {
        // Admit the repaint after earlier resize and parameter operations.
        queue.async { [weak self] in self?.requestFrame(timestamp, surface: surface, animated: false) }
    }

    func tick(_ timestamp: CFTimeInterval, surface: UInt64) {
        requestFrame(timestamp, surface: surface, animated: true)
    }

    private func requestFrame(_ timestamp: CFTimeInterval, surface: UInt64, animated: Bool) {
        pendingLock.lock()
        guard activeSurface == surface else { pendingLock.unlock(); return }
        latestFrameSurface = surface
        latestFrameTimestamp = timestamp
        latestFrameAnimated = animated
        pendingLock.unlock()
        // Coalesce display callbacks and standalone repaints from control changes.
        frameSource.add(data: 1)
    }

    private func drainLatestFrame() {
        pendingLock.lock()
        guard let handle, owner.attached, latestFrameSurface == owner.surfaceGeneration else { pendingLock.unlock(); return }
        let timestamp = latestFrameTimestamp
        let animated = latestFrameAnimated
        latestFrameSurface = nil
        pendingLock.unlock()
        if animated && owner.suspended { return }
        if !animated { _ = still_engine_resume(handle) }
        applyPendingInputs()
        let result = animated ? still_engine_frame_tick_animated(handle, timestamp) : still_engine_frame_tick(handle, timestamp)
        logIfNeeded(result, operation: "frame")
        if !animated && owner.suspended { _ = still_engine_suspend(handle) }
    }

    private func applyPendingInputs() {
        pendingLock.lock()
        let ambient = pendingAmbientFrame
        let updateAmbient = hasPendingAmbientUpdate
        let controls = pendingAmbientControls
        let updateControls = hasPendingControlsUpdate
        let smoothing = pendingAmbientSmoothing
        let updateSmoothing = hasPendingSmoothingUpdate
        let modulation = pendingModulation
        let modulationDeadline = pendingModulationDeadline
        let updateModulation = hasPendingModulationUpdate
        hasPendingAmbientUpdate = false
        hasPendingControlsUpdate = false
        hasPendingSmoothingUpdate = false
        hasPendingModulationUpdate = false
        pendingLock.unlock()
        if let handle {
            let now = ProcessInfo.processInfo.systemUptime
            if updateModulation {
                owner.modulationDeadline = modulationDeadline
                owner.hasModulation = modulation != nil && modulationDeadline > now
                if owner.hasModulation, var modulation { logIfNeeded(still_engine_modulation_update(handle, &modulation), operation: "modulation") }
                else { logIfNeeded(still_engine_modulation_update(handle, nil), operation: "modulation") }
            } else if owner.hasModulation && now >= owner.modulationDeadline {
                owner.hasModulation = false
                logIfNeeded(still_engine_modulation_update(handle, nil), operation: "modulation")
            }
            if updateSmoothing {
                if var smoothing { _ = still_engine_ambient_smoothing_set(handle, &smoothing) }
                else { _ = still_engine_ambient_smoothing_set(handle, nil) }
            }
            if updateControls {
                if var controls { _ = still_engine_ambient_controls_update(handle, &controls) }
                else { _ = still_engine_ambient_controls_update(handle, nil) }
            }
            if updateAmbient {
                if var ambient {
                    _ = still_engine_ambient_update(handle, &ambient)
                } else {
                    _ = still_engine_ambient_update(handle, nil)
                }
            }
        }
    }

    /// One latest slot, including clear. Deadlines prevent replay after a hidden window or stalled producer.
    func updateModulation(_ input: StillModulationInput?, expiresAt: CFTimeInterval) {
        guard input?.isValid != false, expiresAt.isFinite else { return }
        pendingLock.lock()
        pendingModulation = input?.ffi; pendingModulationDeadline = expiresAt
        hasPendingModulationUpdate = true
        pendingLock.unlock()
    }

    func sendPointer(position: CGPoint, velocity: CGVector, active: Bool, surface: UInt64) {
        let event = StillPointerEvent(abi_version: still_abi_version(), struct_size: UInt32(MemoryLayout<StillPointerEvent>.size), x: Float(position.x), y: Float(position.y), velocity_x: Float(velocity.dx), velocity_y: Float(velocity.dy), active: active ? 1 : 0, reserved: 0)
        queue.async { [weak self] in guard let self, let handle, owner.attached, owner.surfaceGeneration == surface else { return }; var input = event; _ = still_engine_input_batch(handle, &input, 1) }
    }

    func setParameter(_ id: String, value: Float) {
        queue.async { [weak self] in guard let handle = self?.handle else { return }; id.withCString { _ = still_engine_parameter_set(handle, $0, value) } }
    }

    func setParameters(_ values: [(String, Float)]) {
        queue.async { [weak self] in
            guard let handle = self?.handle else { return }
            for (id, value) in values { id.withCString { _ = still_engine_parameter_set(handle, $0, value) } }
        }
    }

    /// Future media adapters supply an sRGB palette and an analyzed 0...1
    /// energy envelope. No audio capture or player dependency lives here.
    /// Passing nil releases the override and gently restores manual colors.
    func updateAmbient(colors: [(Float, Float, Float)]?, energy: Float = 0) {
        var frame: StillAmbientFrame?
        if let colors {
            guard colors.count == 3 && energy.isFinite && colors.allSatisfy({ $0.0.isFinite && $0.1.isFinite && $0.2.isFinite }) else { return }
            frame = StillAmbientFrame(abi_version: still_abi_version(), struct_size: UInt32(MemoryLayout<StillAmbientFrame>.size),
                color1: colors[0], color2: colors[1], color3: colors[2], energy: energy)
        }
        // A single latest-value slot bounds producer traffic while rendering
        // is suspended or slower than a media adapter's delivery cadence.
        pendingLock.lock()
        pendingAmbientFrame = frame
        hasPendingAmbientUpdate = true
        pendingAmbientControls = nil
        hasPendingControlsUpdate = false
        pendingLock.unlock()
    }

    /// General snapshot input. Gains/offsets/scales apply relative to the manual
    /// rig; nil smoothly releases all overrides. Latest input wins, without a queue.
    func updateAmbient(lights: [AmbientLightInput]?, brightness: Float = 1) {
        var frame: StillAmbientControlFrame?
        if let lights {
            guard lights.count == 3 && lights.allSatisfy(\.isFinite) && brightness.isFinite else { return }
            frame = StillAmbientControlFrame(abi_version: still_abi_version(), struct_size: UInt32(MemoryLayout<StillAmbientControlFrame>.size),
                lights: (lights[0].ffi, lights[1].ffi, lights[2].ffi), brightness: brightness)
        }
        pendingLock.lock()
        pendingAmbientControls = frame
        hasPendingControlsUpdate = true
        pendingAmbientFrame = nil
        hasPendingAmbientUpdate = false
        pendingLock.unlock()
    }

    /// Code-only response tuning; the app's default stays gentle (0.85 s).
    /// A player adapter may choose fast attacks, slower releases and slow color.
    func configureAmbientSmoothing(colorSeconds: Float = 0.85, attackSeconds: Float = 0.85,
                                   releaseSeconds: Float = 0.85, transformSeconds: Float = 0.85) {
        guard [colorSeconds, attackSeconds, releaseSeconds, transformSeconds].allSatisfy({ $0.isFinite && (0...60).contains($0) }) else { return }
        let settings = StillAmbientSmoothing(abi_version: still_abi_version(), struct_size: UInt32(MemoryLayout<StillAmbientSmoothing>.size),
            color_seconds: colorSeconds, attack_seconds: attackSeconds, release_seconds: releaseSeconds, transform_seconds: transformSeconds)
        pendingLock.lock()
        pendingAmbientSmoothing = settings
        hasPendingSmoothingUpdate = true
        pendingLock.unlock()
    }

    func parameter(_ id: String) -> Float? {
        queue.sync { () -> Float? in
            guard let handle else { return nil }; var value: Float = 0
            let result = id.withCString { still_engine_parameter_get(handle, $0, &value) }
            return result == 0 ? value : nil
        }
    }

    func parameterDescriptors() -> [ParameterPresentation] {
        guard let data = exportBuffer(still_engine_parameter_catalog_export),
              let catalog = try? JSONDecoder().decode(ParameterCatalog.self, from: data) else { return [] }
        return catalog.parameters.map {
            ParameterPresentation(
                id: $0.id,
                name: $0.name,
                range: $0.minimum...$0.maximum,
                defaultValue: $0.defaultValue
            )
        }
    }

    func selectScene(_ id: String) -> Bool {
        queue.sync {
            guard let handle else { return false }
            return id.withCString { still_engine_scene_select(handle, $0) == 0 }
        }
    }

    func sceneCatalog() -> (scenes: [ScenePresentation], selected: String?) {
        guard let data = exportBuffer(still_engine_scene_catalog_export),
              let catalog = try? JSONDecoder().decode(SceneCatalog.self, from: data) else {
            return ([], nil)
        }
        return (catalog.scenes, catalog.selected)
    }

    func newVariation() { queue.async { [weak self] in guard let handle = self?.handle else { return }; _ = still_engine_new_variation(handle) } }

    func setQuality(_ value: UInt32) { queue.async { [weak self] in guard let handle = self?.handle else { return }; _ = still_engine_quality_set(handle, value) } }

    func setDisplayRefresh(_ hertz: Float, surface: UInt64) { queue.async { [weak self] in guard let self, let handle, owner.attached, owner.surfaceGeneration == surface else { return }; _ = still_engine_display_refresh_set(handle, hertz) } }

    func setPowerState(lowPower: Bool, thermalLevel: UInt32, surface: UInt64) {
        queue.async { [weak self] in guard let self, let handle, owner.attached, owner.surfaceGeneration == surface else { return }; _ = still_engine_power_state_set(handle, lowPower ? 1 : 0, thermalLevel) }
    }

    func selectPreset(_ id: String) -> Bool {
        queue.sync { guard let handle else { return false }; return id.withCString { still_engine_preset_select(handle, $0) == 0 } }
    }

    func saveUserPreset(named name: String) -> Bool {
        queue.sync { guard let handle else { return false }; return name.withCString { still_engine_user_preset_save(handle, $0) == 0 } }
    }

    func deleteUserPreset(named name: String) -> Bool {
        queue.sync { guard let handle else { return false }; return name.withCString { still_engine_user_preset_delete(handle, $0) == 0 } }
    }

    func presetCatalog() -> (presets: [PresetPresentation], selected: String?) {
        guard let data = exportBuffer(still_engine_preset_catalog_export), let catalog = try? JSONDecoder().decode(PresetCatalog.self, from: data) else { return ([], nil) }
        return (catalog.presets, catalog.selected)
    }

    func importState(_ data: Data) -> Bool {
        queue.sync {
            guard let handle else { return false }
            return data.withUnsafeBytes { bytes in
                guard let base = bytes.baseAddress?.assumingMemoryBound(to: UInt8.self) else { return false }
                return still_engine_state_import(handle, base, data.count) == 0
            }
        }
    }

    func exportState() -> Data? {
        exportBuffer(still_engine_state_export)
    }

    func exportMetrics() -> Data? {
        exportBuffer(still_engine_metrics_export)
    }

    private func exportBuffer(_ function: (OpaquePointer?, UnsafeMutablePointer<UInt8>?, Int, UnsafeMutablePointer<Int>?) -> Int32) -> Data? {
        queue.sync { () -> Data? in
            guard let handle else { return nil }; var length = 0
            _ = function(handle, nil, 0, &length)
            guard length > 0 else { return nil }
            var data = Data(count: length)
            let result = data.withUnsafeMutableBytes { bytes in function(handle, bytes.baseAddress?.assumingMemoryBound(to: UInt8.self), length, &length) }
            return result == 0 ? data : nil
        }
    }

    private func logIfNeeded(_ result: Int32, operation: String) {
        if operation == "frame" {
            owner.frameFailures = result == 0 ? 0 : min(8, owner.frameFailures + 1)
        }
        guard result != 0, let handle else { return }
        var length = 0; _ = still_engine_error_drain(handle, nil, 0, &length)
        guard length > 1 else { return }
        var bytes = [CChar](repeating: 0, count: length)
        if still_engine_error_drain(handle, &bytes, length, &length) == 0 {
            let message = String(decoding: bytes.prefix { $0 != 0 }.map { UInt8(bitPattern: $0) }, as: UTF8.self)
            NSLog("Still \(operation): \(message)")
            if !owner.reportedFailure && (result == 5 || operation == "surface attach" || message.localizedCaseInsensitiveContains("memory") || owner.frameFailures >= 8) {
                owner.reportedFailure = true
                onError?("This space is unavailable. Try preparing it again.")
            }
        }
    }
}
