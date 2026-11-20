use bytemuck::{Pod, Zeroable};
use std::{ffi::c_void, num::NonZeroU64};

mod platform;

pub use wgpu;

#[derive(Debug, thiserror::Error)]
pub enum RendererError {
    #[error("no adapter is available for the platform GPU backend")]
    NoAdapter,
    #[error("Still has no GPU backend configured for this platform")]
    UnsupportedPlatform,
    #[error(
        "the AppKit surface entry point is only supported on macOS; Windows requires SwapChainPanel interop"
    )]
    UnsupportedSurface,
    #[error("the selected adapter cannot present to this surface")]
    IncompatibleSurface,
    #[error("device creation failed: {0}")]
    Device(#[from] wgpu::RequestDeviceError),
    #[error("surface creation failed: {0}")]
    Surface(#[from] wgpu::CreateSurfaceError),
    #[error("surface is not attached")]
    Detached,
    #[error("surface is suspended or has zero size")]
    Suspended,
    #[error("surface error: {0}")]
    Frame(#[from] wgpu::SurfaceError),
    #[error("compositor source is not prepared")]
    CompositorUnprepared,
}

pub struct AcquiredFrame {
    pub texture: wgpu::SurfaceTexture,
    pub view: wgpu::TextureView,
}

/// Owns global wgpu objects and the presentation surface. Native surface creation
/// is isolated in `platform`; scenes use only the shared device and queue.
pub struct Renderer {
    _instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    surface: Option<wgpu::Surface<'static>>,
    config: Option<wgpu::SurfaceConfiguration>,
    size: [u32; 2],
    suspended: bool,
    compositor: Option<Compositor>,
}

impl Renderer {
    pub fn new() -> Result<Self, RendererError> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: platform::backends()?,
            ..Default::default()
        });
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .ok_or(RendererError::NoAdapter)?;
        let limits = wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits());
        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("Still device"),
                required_features: wgpu::Features::empty(),
                required_limits: limits,
            },
            None,
        ))?;
        Ok(Self {
            _instance: instance,
            adapter,
            device,
            queue,
            surface: None,
            config: None,
            size: [0, 0],
            suspended: false,
            compositor: None,
        })
    }

    /// Attach to an AppKit view backed by a `CAMetalLayer`.
    ///
    /// # Safety
    ///
    /// `ns_view` must be a valid retained `NSView` whose backing layer is a
    /// `CAMetalLayer`. The caller must invoke [`Self::detach`] before releasing
    /// either the view or its layer, and must serialize calls to this renderer.
    pub unsafe fn attach(
        &mut self,
        ns_view: *mut c_void,
        width: u32,
        height: u32,
    ) -> Result<wgpu::TextureFormat, RendererError> {
        self.detach();
        let surface = unsafe { platform::appkit_surface(&self._instance, ns_view)? };
        let caps = surface.get_capabilities(&self.adapter);
        if caps.formats.is_empty() || caps.present_modes.is_empty() || caps.alpha_modes.is_empty() {
            return Err(RendererError::IncompatibleSurface);
        }
        let format = caps
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .or_else(|| caps.formats.first().copied())
            .unwrap_or(wgpu::TextureFormat::Bgra8UnormSrgb);
        let present_mode = if caps.present_modes.contains(&wgpu::PresentMode::Fifo) {
            wgpu::PresentMode::Fifo
        } else {
            caps.present_modes[0]
        };
        let alpha_mode = caps
            .alpha_modes
            .iter()
            .copied()
            .find(|m| *m == wgpu::CompositeAlphaMode::Opaque)
            .unwrap_or(caps.alpha_modes[0]);
        self.surface = Some(surface);
        self.config = Some(wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: width.max(1),
            height: height.max(1),
            present_mode,
            desired_maximum_frame_latency: 2,
            alpha_mode,
            view_formats: vec![],
        });
        self.compositor = Some(Compositor::new(&self.device, format));
        self.resize(width, height);
        Ok(format)
    }

    pub fn detach(&mut self) {
        self.config = None;
        self.surface = None;
        self.compositor = None;
        self.size = [0, 0];
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.size = [width, height];
        if width == 0 || height == 0 {
            return;
        }
        if let (Some(surface), Some(config)) = (&self.surface, &mut self.config) {
            config.width = width;
            config.height = height;
            surface.configure(&self.device, config);
        }
    }

    pub fn set_suspended(&mut self, value: bool) {
        self.suspended = value;
    }
    pub fn is_renderable(&self) -> bool {
        !self.suspended && self.surface.is_some() && self.size[0] > 0 && self.size[1] > 0
    }
    pub fn size(&self) -> [u32; 2] {
        self.size
    }
    pub fn surface_format(&self) -> Option<wgpu::TextureFormat> {
        self.config.as_ref().map(|c| c.format)
    }

    pub fn acquire(&mut self) -> Result<AcquiredFrame, RendererError> {
        if self.suspended || self.size.contains(&0) {
            return Err(RendererError::Suspended);
        }
        let surface = self.surface.as_ref().ok_or(RendererError::Detached)?;
        match surface.get_current_texture() {
            Ok(texture) => {
                let view = texture
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());
                Ok(AcquiredFrame { texture, view })
            }
            Err(error) if surface_error_action(&error) == SurfaceErrorAction::Reconfigure => {
                if let Some(config) = &self.config {
                    surface.configure(&self.device, config);
                }
                let texture = surface.get_current_texture()?;
                let view = texture
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());
                Ok(AcquiredFrame { texture, view })
            }
            Err(e) => Err(RendererError::Frame(e)),
        }
    }

    pub fn create_encoder(&self) -> wgpu::CommandEncoder {
        self.device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Still frame"),
            })
    }

    /// Reconfigures the swapchain after an error or a panic contained by the
    /// C ABI. This prevents one abandoned surface image from poisoning all
    /// subsequent display callbacks.
    pub fn recover_after_frame_failure(&mut self) {
        if self.size.contains(&0) {
            return;
        }
        if let (Some(surface), Some(config)) = (&self.surface, &self.config) {
            surface.configure(&self.device, config);
        }
    }

    pub fn prepare_compositor_sources(
        &mut self,
        source_a: (u64, &wgpu::TextureView),
        source_b: (u64, &wgpu::TextureView),
    ) -> Result<(), RendererError> {
        let compositor = self.compositor.as_mut().ok_or(RendererError::Detached)?;
        compositor.prepare_sources(&self.device, source_a, source_b);
        Ok(())
    }

    pub fn release_compositor_sources(&mut self) {
        if let Some(compositor) = &mut self.compositor {
            compositor.sources.clear();
            compositor.active_source = None;
        }
    }

    pub fn encode_composite(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        exposure_a: f32,
        exposure_b: f32,
        blend: f32,
    ) -> Result<(), RendererError> {
        let compositor = self.compositor.as_ref().ok_or(RendererError::Detached)?;
        compositor.encode(&self.queue, encoder, target, exposure_a, exposure_b, blend)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SurfaceErrorAction {
    Reconfigure,
    RetryLater,
    Fatal,
}

fn surface_error_action(error: &wgpu::SurfaceError) -> SurfaceErrorAction {
    match error {
        wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated => SurfaceErrorAction::Reconfigure,
        wgpu::SurfaceError::Timeout => SurfaceErrorAction::RetryLater,
        wgpu::SurfaceError::OutOfMemory => SurfaceErrorAction::Fatal,
    }
}

pub fn padded_uniform_size<T>() -> NonZeroU64 {
    NonZeroU64::new(std::mem::size_of::<T>() as u64).expect("uniforms are non-empty")
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct CompositeUniforms {
    exposure_a: f32,
    exposure_b: f32,
    blend: f32,
    _padding: f32,
}

struct Compositor {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    uniforms: wgpu::Buffer,
    // Two entries cover alternating history views without per-frame GPU
    // allocations. The runtime clears them when scene ownership changes.
    sources: Vec<((u64, u64), wgpu::BindGroup)>,
    active_source: Option<usize>,
}

impl Compositor {
    fn new(device: &wgpu::Device, output_format: wgpu::TextureFormat) -> Self {
        use wgpu::util::DeviceExt;
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Still compositor layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Still compositor sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let uniforms = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Still compositor uniforms"),
            contents: bytemuck::bytes_of(&CompositeUniforms::zeroed()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Still compositor shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("compositor.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Still compositor pipeline layout"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Still compositor pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: "fullscreen_vertex",
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: "composite_fragment",
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: output_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });
        Self {
            pipeline,
            layout,
            sampler,
            uniforms,
            sources: Vec::with_capacity(2),
            active_source: None,
        }
    }

    fn prepare_sources(
        &mut self,
        device: &wgpu::Device,
        source_a: (u64, &wgpu::TextureView),
        source_b: (u64, &wgpu::TextureView),
    ) {
        let keys = (source_a.0, source_b.0);
        if let Some(index) = self.sources.iter().position(|(key, _)| *key == keys) {
            self.active_source = Some(index);
            return;
        }
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Still compositor source"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(source_a.1),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(source_b.1),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: self.uniforms.as_entire_binding(),
                },
            ],
        });
        // A viewport/quality change replaces resources. Evict the old pair
        // instead of retaining a growing set of texture references.
        if self.sources.len() == 2 {
            self.sources.clear();
        }
        self.sources.push((keys, group));
        self.active_source = Some(self.sources.len() - 1);
    }

    fn encode(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        exposure_a: f32,
        exposure_b: f32,
        blend: f32,
    ) -> Result<(), RendererError> {
        let group = self
            .active_source
            .and_then(|index| self.sources.get(index).map(|(_, group)| group))
            .ok_or(RendererError::CompositorUnprepared)?;
        queue.write_buffer(
            &self.uniforms,
            0,
            bytemuck::bytes_of(&CompositeUniforms {
                exposure_a,
                exposure_b,
                blend,
                _padding: 0.0,
            }),
        );
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Still composite"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, group, &[]);
        pass.draw(0..3, 0..1);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_fullscreen_zero_size_and_detach_transitions_are_safe() {
        let Ok(mut renderer) = Renderer::new() else {
            return;
        };
        for _ in 0..8 {
            renderer.resize(1280, 720);
            assert_eq!(renderer.size(), [1280, 720]);
            renderer.resize(0, 0);
            assert_eq!(renderer.size(), [0, 0]);
            renderer.resize(2560, 1600);
            assert_eq!(renderer.size(), [2560, 1600]);
            renderer.resize(800, 600);
            assert_eq!(renderer.size(), [800, 600]);
        }
        renderer.detach();
        renderer.detach();
        assert!(!renderer.is_renderable());
    }

    #[test]
    fn surface_recovery_policy_is_explicit() {
        assert_eq!(
            surface_error_action(&wgpu::SurfaceError::Lost),
            SurfaceErrorAction::Reconfigure
        );
        assert_eq!(
            surface_error_action(&wgpu::SurfaceError::Outdated),
            SurfaceErrorAction::Reconfigure
        );
        assert_eq!(
            surface_error_action(&wgpu::SurfaceError::Timeout),
            SurfaceErrorAction::RetryLater
        );
        assert_eq!(
            surface_error_action(&wgpu::SurfaceError::OutOfMemory),
            SurfaceErrorAction::Fatal
        );
    }

    #[test]
    fn compositor_shader_and_pipeline_validate() {
        let source = include_str!("compositor.wgsl");
        let module = naga::front::wgsl::parse_str(source).expect("compositor WGSL parses");
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .expect("compositor WGSL validates");
        if let Ok(renderer) = Renderer::new() {
            assert_eq!(std::mem::size_of::<CompositeUniforms>(), 16);
            let mut compositor =
                Compositor::new(&renderer.device, wgpu::TextureFormat::Bgra8UnormSrgb);
            let source = renderer.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Compositor test source"),
                size: wgpu::Extent3d {
                    width: 8,
                    height: 8,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba16Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            let target = renderer.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Compositor test target"),
                size: wgpu::Extent3d {
                    width: 8,
                    height: 8,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Bgra8UnormSrgb,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let readback = renderer.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Compositor test readback"),
                size: 256 * 8,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            let source_view = source.create_view(&Default::default());
            let target_view = target.create_view(&Default::default());
            compositor.prepare_sources(&renderer.device, (1, &source_view), (1, &source_view));
            let mut encoder = renderer.create_encoder();
            {
                let _clear = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Compositor test clear"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &source_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 0.2,
                                g: 0.3,
                                b: 0.4,
                                a: 1.0,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
            }
            compositor
                .encode(&renderer.queue, &mut encoder, &target_view, 0.5, 0.5, 0.0)
                .expect("compositor encodes");
            encoder.copy_texture_to_buffer(
                wgpu::ImageCopyTexture {
                    texture: &target,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::ImageCopyBuffer {
                    buffer: &readback,
                    layout: wgpu::ImageDataLayout {
                        offset: 0,
                        bytes_per_row: Some(256),
                        rows_per_image: Some(8),
                    },
                },
                wgpu::Extent3d {
                    width: 8,
                    height: 8,
                    depth_or_array_layers: 1,
                },
            );
            renderer.queue.submit(Some(encoder.finish()));
            let slice = readback.slice(..);
            let (sender, receiver) = std::sync::mpsc::channel();
            slice.map_async(wgpu::MapMode::Read, move |result| {
                sender.send(result).expect("map result receiver exists");
            });
            renderer.device.poll(wgpu::Maintain::Wait);
            receiver
                .recv()
                .expect("map callback runs")
                .expect("readback maps");
            let bytes = slice.get_mapped_range();
            assert!(bytes[..3].iter().any(|channel| *channel > 0));
            drop(bytes);
            readback.unmap();
        }
    }
}
