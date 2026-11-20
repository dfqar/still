import SwiftUI
import AppKit

struct ContentView: View {
    @ObservedObject var model: AppModel
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var presetName = ""

    var body: some View {
        ZStack(alignment: .topTrailing) {
            if let engine = model.engine {
                MetalCanvas(engine: engine, frameRatePreference: model.frameRatePreference, powerPolicy: model.powerPolicy, activity: model.noteActivity, becameInactive: model.applicationBecameInactive)
                    .id(ObjectIdentifier(engine))
                    .ignoresSafeArea()
                    .accessibilityLabel("\(selectedSceneName) procedural scene")

                controls
                    .padding(22)
                    .opacity(model.controlsVisible ? 1 : 0)
                    .allowsHitTesting(model.controlsVisible)
                    .accessibilityHidden(!model.controlsVisible)
                    .animation(reduceMotion ? nil : .easeInOut(duration: 0.45), value: model.controlsVisible)
            } else if let error = model.renderingError {
                VStack(spacing: 12) {
                    Text("Rendering unavailable").font(.headline)
                    Text(error).foregroundStyle(.secondary).multilineTextAlignment(.center)
                    Button("Retry", action: model.retry)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .padding(32)
            } else {
                ProgressView("Preparing renderer…")
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .background(Color.black)
        .onHover { if $0 { model.noteActivity() } }
        .alert("Save Current Preset", isPresented: $model.showingSavePreset) {
            TextField("Preset name", text: $presetName)
            Button("Cancel", role: .cancel) { presetName = "" }
            Button("Save") { if model.savePreset(named: presetName) { presetName = "" } }
                .disabled(presetName.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
        } message: {
            Text("Saves the current seed and \(selectedSceneName) parameters.")
        }
    }

    private var controls: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 14) {
                HStack {
                    Text("STILL").font(.system(size: 11, weight: .semibold, design: .rounded)).tracking(2.2)
                    Spacer()
                    Button(action: model.newVariation) { Image(systemName: "shuffle") }
                        .buttonStyle(.plain).accessibilityLabel("New Variation")
                        .help("New Variation (⌘N)")
                }


                VStack(alignment: .leading, spacing: 6) {
                    Text("Space").font(.system(size: 11, weight: .medium)).foregroundStyle(.secondary)
                    Picker("Space", selection: sceneBinding) {
                        ForEach(model.scenes) { scene in Text(scene.name).tag(Optional(scene.id)) }
                    }
                    .labelsHidden()
                    .accessibilityLabel("Procedural space")
                }

                VStack(alignment: .leading, spacing: 6) {
                    Text("Preset").font(.system(size: 11, weight: .medium)).foregroundStyle(.secondary)
                    HStack(spacing: 8) {
                        Picker("Preset", selection: presetBinding) {
                            Text("Custom").tag(String?.none)
                            ForEach(model.presets) { preset in Text(preset.name).tag(Optional(preset.id)) }
                        }
                        .labelsHidden().accessibilityLabel("\(selectedSceneName) preset")
                        Button { model.showingSavePreset = true; model.noteActivity() } label: { Image(systemName: "plus") }
                            .buttonStyle(.plain).accessibilityLabel("Save current preset")
                        if model.selectedPreset?.hasPrefix("user.") == true {
                            Button(role: .destructive, action: model.deleteSelectedUserPreset) { Image(systemName: "trash") }
                                .buttonStyle(.plain).accessibilityLabel("Delete selected preset")
                        }
                    }
                }

                Divider().overlay(.white.opacity(0.08))

                if model.selectedScene == "frost" {
                    frostControls
                } else {
                    ForEach(model.descriptors) { parameterControl($0) }
                }

                Divider().overlay(.white.opacity(0.08))

                VStack(spacing: 8) {
                    HStack(spacing: 10) {
                        Picker("Quality", selection: Binding(get: { model.quality }, set: model.setQuality)) {
                            Text("Auto").tag(UInt32(0)); Text("Eco").tag(UInt32(1)); Text("Balanced").tag(UInt32(2)); Text("High").tag(UInt32(3))
                        }
                        .accessibilityLabel("Rendering quality")
                        Picker("Frame rate", selection: Binding(get: { model.frameRatePreference }, set: model.setFrameRatePreference)) {
                            Text("Auto").tag(0)
                            Text("30 Hz").tag(30)
                            Text("60 Hz").tag(60)
                            Text("Native").tag(120)
                        }
                        .accessibilityLabel("Frame rate preference")
                    }
                    Picker("Power", selection: Binding(get: { model.powerPolicy }, set: model.setPowerPolicy)) {
                        Text("Automatic").tag(0); Text("Quality First").tag(1); Text("Efficiency").tag(2)
                    }
                    .accessibilityLabel("Power policy")
                    .frame(maxWidth: .infinity)
                }
                .labelsHidden().controlSize(.small)
            }
            .padding(16)
        }
        .scrollIndicators(.hidden)
        .frame(width: 274)
        .frame(maxHeight: 590)
        .background(.black.opacity(0.66), in: RoundedRectangle(cornerRadius: 14, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 14, style: .continuous).stroke(.white.opacity(0.09), lineWidth: 0.5))
        .shadow(color: .black.opacity(0.35), radius: 24, y: 8)
        .accessibilityElement(children: .contain)
        .accessibilityLabel("\(selectedSceneName) controls")
    }

    private var presetBinding: Binding<String?> {
        Binding(get: { model.selectedPreset }, set: { if let id = $0 { model.selectPreset(id) } })
    }

    private func parameterControl(_ descriptor: ParameterPresentation, label: String? = nil) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text(label ?? descriptor.name).font(.system(size: 11, weight: .medium)).foregroundStyle(.secondary)
                Spacer()
                Text(model.values[descriptor.id, default: descriptor.defaultValue], format: .number.precision(.fractionLength(2)))
                    .font(.system(size: 10, design: .monospaced)).foregroundStyle(.tertiary)
            }
            Slider(value: Binding(get: { model.values[descriptor.id] ?? descriptor.defaultValue }, set: { model.set(descriptor.id, to: $0) }), in: descriptor.range)
                .tint(Color.white.opacity(0.72))
                .accessibilityLabel(label ?? descriptor.name)
                .accessibilityValue(Text(model.values[descriptor.id, default: descriptor.defaultValue], format: .number.precision(.fractionLength(2))))
        }
    }

    private var frostControls: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("LIGHT BEHIND GLASS")
                .font(.system(size: 9, weight: .semibold)).tracking(1.4).foregroundStyle(.tertiary)
            ForEach(model.descriptors.filter { ["frost.brightness", "frost.motion"].contains($0.id) }) { parameterControl($0) }
            ForEach(1...3, id: \.self) { index in
                VStack(alignment: .leading, spacing: 10) {
                    ColorPicker("Light \(index)", selection: lightColor(index), supportsOpacity: false)
                        .font(.system(size: 11, weight: .medium))
                        .accessibilityLabel("Light \(index) color")
                    if let power = model.descriptors.first(where: { $0.id == "frost.light\(index).power" }) {
                        parameterControl(power, label: "Light \(index) intensity")
                    }
                    DisclosureGroup("Position & shape") {
                        VStack(spacing: 10) {
                            ForEach(model.descriptors.filter { $0.id.hasPrefix("frost.light\(index).") && !["red", "green", "blue", "power"].contains($0.id.components(separatedBy: ".").last ?? "") }) {
                                parameterControl($0, label: "Light \(index) \($0.name.lowercased())")
                            }
                        }.padding(.top, 8)
                    }
                    .font(.system(size: 11)).foregroundStyle(.secondary)
                }
                .padding(10)
                .background(.white.opacity(0.035), in: RoundedRectangle(cornerRadius: 9))
            }
            DisclosureGroup("Glass material") {
                VStack(spacing: 12) {
                    ForEach(model.descriptors.filter { ["frost.diffusion", "frost.frost", "frost.grain", "frost.ambient"].contains($0.id) }) { parameterControl($0) }
                }.padding(.top, 10)
            }
            .font(.system(size: 11)).foregroundStyle(.secondary)
        }
    }

    private func lightColor(_ index: Int) -> Binding<Color> {
        let prefix = "frost.light\(index)."
        return Binding(get: {
            Color(.sRGB, red: model.values[prefix + "red", default: 0],
                  green: model.values[prefix + "green", default: 0],
                  blue: model.values[prefix + "blue", default: 0], opacity: 1)
        }, set: { color in
            guard let rgb = NSColor(color).usingColorSpace(.sRGB) else { return }
            model.setColor(prefix: prefix, red: rgb.redComponent, green: rgb.greenComponent, blue: rgb.blueComponent)
        })
    }

    private var sceneBinding: Binding<String?> {
        Binding(get: { model.selectedScene }, set: { if let id = $0 { model.selectScene(id) } })
    }

    private var selectedSceneName: String {
        model.scenes.first(where: { $0.id == model.selectedScene })?.name ?? "Still"
    }
}
