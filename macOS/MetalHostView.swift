import SwiftUI
import AppKit
import QuartzCore
import Metal

struct MetalCanvas: NSViewRepresentable {
    let engine: EngineController
    let frameRatePreference: Int
    let powerPolicy: Int
    let activity: @MainActor () -> Void
    let becameInactive: @MainActor () -> Void
    var contentRevision: UInt64 = 0
    var placeholderColor: CGColor? = nil

    func makeNSView(context: Context) -> MetalHostView {
        let view = MetalHostView(engine: engine, placeholderColor: placeholderColor)
        view.setFrameRatePreference(frameRatePreference)
        view.setPowerPolicy(powerPolicy)
        view.activity = activity
        view.becameInactive = becameInactive
        view.setContentRevision(contentRevision)
        return view
    }
    func updateNSView(_ nsView: MetalHostView, context: Context) {
        nsView.setFrameRatePreference(frameRatePreference)
        nsView.setPowerPolicy(powerPolicy)
        nsView.setPlaceholderColor(placeholderColor)
        nsView.setContentRevision(contentRevision)
    }
    static func dismantleNSView(_ nsView: MetalHostView, coordinator: ()) { nsView.shutdown() }
}

final class MetalHostView: NSView {
    let engine: EngineController
    var activity: (@MainActor () -> Void)?
    var becameInactive: (@MainActor () -> Void)?
    private var displayLink: CADisplayLink?
    private var surfaceToken: UInt64?
    private var attached: Bool { surfaceToken != nil }
    private var systemSleeping = false
    private var lastPhysicalSize: CGSize?
    private var lastSuspended: Bool?
    private var needsStillFrame = true
    private var contentRevision: UInt64 = 0
    private var placeholderColor: CGColor?
    private var lastRefreshRate: Float?
    private var lastPowerState: (Bool, UInt32)?
    private var wasInteractive = false
    var isAnimating: Bool { displayLink != nil && displayLink?.isPaused == false }
    var isSurfaceAttached: Bool { attached }
    private var observers: [NSObjectProtocol] = []
    private var workspaceObservers: [NSObjectProtocol] = []
    private var previousPointer: (point: CGPoint, time: TimeInterval)?
    private var frameRatePreference = 0
    private var powerPolicy = 0
    private let profileWhileInactive = ProcessInfo.processInfo.environment["STILL_PROFILE_WHEN_INACTIVE"] == "1"
    var allocatedMetalBytes: Int? { (layer as? CAMetalLayer)?.device?.currentAllocatedSize }
    /// Rare snapshot changes get one paint in a visible inactive window. Hidden
    /// windows retain the revision until reveal; this never starts a GPU loop.
    func setContentRevision(_ value: UInt64) {
        guard value != contentRevision else { return }
        contentRevision = value; needsStillFrame = true; updateVisibility()
    }

    init(engine: EngineController, placeholderColor: CGColor? = nil) {
        self.engine = engine
        self.placeholderColor = placeholderColor
        super.init(frame: .zero)
        wantsLayer = true
    }
    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }

    override func makeBackingLayer() -> CALayer {
        let layer = CAMetalLayer()
        layer.pixelFormat = .bgra8Unorm_srgb
        layer.framebufferOnly = true
        layer.isOpaque = true
        layer.backgroundColor = placeholderColor ?? NSColor.black.cgColor
        return layer
    }

    /// Cover the interval before the first drawable without changing surface visibility.
    func setPlaceholderColor(_ color: CGColor?) {
        guard placeholderColor != color else { return }
        placeholderColor = color
        layer?.backgroundColor = color ?? NSColor.black.cgColor
    }

    override var acceptsFirstResponder: Bool { true }
    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        trackingAreas.forEach(removeTrackingArea)
        addTrackingArea(NSTrackingArea(rect: .zero, options: [.mouseMoved, .mouseEnteredAndExited, .activeAlways, .inVisibleRect], owner: self))
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        shutdown()
        guard window != nil else { return }
        updateMetalSize()
        surfaceToken = engine.attach(view: self, physicalSize: physicalSize)
        guard attached else { return }
        lastPhysicalSize = physicalSize
        needsStillFrame = true
        installObservers()
        startDisplayLink()
        updateVisibility()
        window?.acceptsMouseMovedEvents = true
    }

    override func layout() { super.layout(); updateVisibility() }
    override func viewDidChangeBackingProperties() { super.viewDidChangeBackingProperties(); updateVisibility() }
    override func viewDidHide() { super.viewDidHide(); updateVisibility() }
    override func viewDidUnhide() { super.viewDidUnhide(); needsStillFrame = true; updateVisibility() }

    override func mouseMoved(with event: NSEvent) {
        let point = convert(event.locationInWindow, from: nil)
        let normalized = CGPoint(x: bounds.width > 0 ? point.x / bounds.width * 2 - 1 : 0, y: bounds.height > 0 ? point.y / bounds.height * 2 - 1 : 0)
        let now = ProcessInfo.processInfo.systemUptime
        var velocity = CGVector.zero
        if let previousPointer {
            let dt = max(now - previousPointer.time, 1.0 / 240.0)
            velocity = CGVector(dx: (normalized.x - previousPointer.point.x) / dt, dy: (normalized.y - previousPointer.point.y) / dt)
        }
        previousPointer = (normalized, now)
        if let token = surfaceToken { engine.sendPointer(position: normalized, velocity: velocity, active: true, surface: token) }
        Task { @MainActor [weak self] in self?.activity?() }
    }
    override func mouseExited(with event: NSEvent) {
        if let token = surfaceToken { engine.sendPointer(position: .zero, velocity: .zero, active: false, surface: token) }
        previousPointer = nil
    }
    override func keyDown(with event: NSEvent) { Task { @MainActor [weak self] in self?.activity?() }; super.keyDown(with: event) }

    func shutdown() {
        displayLink?.invalidate(); displayLink = nil
        removeObservers()
        if let token = surfaceToken { engine.detach(surface: token); surfaceToken = nil }
        lastPhysicalSize = nil; lastSuspended = nil; needsStillFrame = true
        lastRefreshRate = nil; lastPowerState = nil
        previousPointer = nil; wasInteractive = false
    }

    func setFrameRatePreference(_ value: Int) {
        guard frameRatePreference != value else { return }
        frameRatePreference = value
        updateRefreshRate()
    }

    func setPowerPolicy(_ value: Int) {
        guard powerPolicy != value else { return }
        powerPolicy = value
        updateRefreshRate()
    }

    @objc private func displayLinkFired(_ link: CADisplayLink) {
        if let token = surfaceToken { engine.tick(link.timestamp, surface: token) }
    }
    private func startDisplayLink() {
        displayLink?.invalidate()
        let link = displayLink(target: self, selector: #selector(displayLinkFired(_:)))
        link.isPaused = true
        link.add(to: .main, forMode: .common)
        displayLink = link
    }

    private var preferredRefreshRate: Float {
        let native = Float(max(1, min(window?.screen?.maximumFramesPerSecond ?? 60, 120)))
        if frameRatePreference == 30 { return min(30, native) }
        if frameRatePreference == 60 { return min(60, native) }
        let process = ProcessInfo.processInfo
        let thermalPressure = process.thermalState == .serious || process.thermalState == .critical
        if powerPolicy == 2 || (powerPolicy == 0 && (process.isLowPowerModeEnabled || thermalPressure)) { return min(60, native) }
        return native
    }

    private func updateRefreshRate() {
        guard let token = surfaceToken else { return }
        let preferred = preferredRefreshRate
        if lastRefreshRate != preferred {
            lastRefreshRate = preferred
            displayLink?.preferredFrameRateRange = CAFrameRateRange(minimum: min(30, preferred), maximum: preferred, preferred: preferred)
            engine.setDisplayRefresh(preferred, surface: token)
        }
        let process = ProcessInfo.processInfo
        let lowPower = powerPolicy == 2 || (powerPolicy == 0 && process.isLowPowerModeEnabled)
        let thermal: UInt32
        switch process.thermalState {
        case .nominal: thermal = 0
        case .fair: thermal = 1
        case .serious: thermal = 2
        case .critical: thermal = 3
        @unknown default: thermal = 1
        }
        if lastPowerState?.0 != lowPower || lastPowerState?.1 != thermal {
            lastPowerState = (lowPower, thermal)
            engine.setPowerState(lowPower: lowPower, thermalLevel: thermal, surface: token)
        }
    }

    private var physicalSize: CGSize {
        let scale = window?.backingScaleFactor ?? NSScreen.main?.backingScaleFactor ?? 1
        return CGSize(width: (bounds.width * scale).rounded(), height: (bounds.height * scale).rounded())
    }
    private func updateMetalSize() {
        guard let metal = layer as? CAMetalLayer else { return }
        let scale = window?.backingScaleFactor ?? NSScreen.main?.backingScaleFactor ?? 1
        if metal.contentsScale != scale { metal.contentsScale = scale }
        if metal.drawableSize != physicalSize { metal.drawableSize = physicalSize }
    }

    private func updateSurfaceSize() {
        updateMetalSize()
        guard let token = surfaceToken, lastPhysicalSize != physicalSize else { return }
        lastPhysicalSize = physicalSize
        needsStillFrame = true
        engine.resize(physicalSize, surface: token)
    }

    private func installObservers() {
        guard let window else { return }
        let center = NotificationCenter.default
        for name in [NSWindow.didMiniaturizeNotification, NSWindow.didBecomeKeyNotification, NSWindow.didChangeOcclusionStateNotification, NSWindow.didChangeScreenNotification, NSWindow.didChangeScreenProfileNotification, NSApplication.didBecomeActiveNotification, NSApplication.didResignActiveNotification, Notification.Name.NSProcessInfoPowerStateDidChange, ProcessInfo.thermalStateDidChangeNotification] {
            observers.append(center.addObserver(forName: name, object: name.rawValue.hasPrefix("NSWindow") ? window : nil, queue: .main) { [weak self] _ in MainActor.assumeIsolated { self?.updateVisibility() } })
        }
        observers.append(center.addObserver(forName: NSWindow.didDeminiaturizeNotification, object: window, queue: .main) { [weak self, weak window] _ in
            // Rebind to the restored window after AppKit finishes changing visibility.
            DispatchQueue.main.async {
                guard let self, let window, self.window === window, self.attached else { return }
                self.needsStillFrame = true
                self.lastRefreshRate = nil
                self.startDisplayLink()
                self.updateVisibility()
            }
        })
        let workspaceCenter = NSWorkspace.shared.notificationCenter
        workspaceObservers.append(workspaceCenter.addObserver(forName: NSWorkspace.willSleepNotification, object: nil, queue: .main) { [weak self] _ in MainActor.assumeIsolated { self?.suspendForSystemSleep() } })
        workspaceObservers.append(workspaceCenter.addObserver(forName: NSWorkspace.didWakeNotification, object: nil, queue: .main) { [weak self] _ in MainActor.assumeIsolated { self?.systemSleeping = false; self?.needsStillFrame = true; self?.updateVisibility() } })
    }
    private func removeObservers() {
        observers.forEach(NotificationCenter.default.removeObserver); observers.removeAll()
        workspaceObservers.forEach(NSWorkspace.shared.notificationCenter.removeObserver); workspaceObservers.removeAll()
    }
    private func suspendForSystemSleep() {
        systemSleeping = true
        updateVisibility()
    }
    private func updateVisibility() {
        updateSurfaceSize(); updateRefreshRate()
        guard let token = surfaceToken else { return }
        let visible = window?.isVisible == true && window?.isMiniaturized == false
            && !isHiddenOrHasHiddenAncestor && !systemSleeping && physicalSize.width > 0 && physicalSize.height > 0
            && window?.occlusionState.contains(.visible) == true
        let animate = visible && (profileWhileInactive || NSApp.isActive)
        displayLink?.isPaused = !animate
        if lastSuspended != !animate { lastSuspended = !animate; engine.setSuspended(!animate, surface: token) }
        if visible && !animate && needsStillFrame {
            needsStillFrame = false
            engine.renderStillFrame(ProcessInfo.processInfo.systemUptime, surface: token)
        } else if animate { needsStillFrame = false }
        let interactive = visible && NSApp.isActive
        if wasInteractive && !interactive { becameInactive?() }
        wasInteractive = interactive
    }
}
