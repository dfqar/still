//! Offline Metal visual QA. Readback is confined to this development example;
//! the application never maps scene buffers or textures.
use std::{env, fs, path::PathBuf};
use still_core::{
    AmbientControlFrame, AmbientFrame, AmbientLightControl, AmbientSmoothing, FramePlan,
    PointerInput, Quality,
};
use still_renderer::{
    Renderer,
    wgpu::{self, util::DeviceExt},
};
use still_scenes::{SCENE_DESCRIPTORS, create_scene};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let destination = PathBuf::from(env::args().nth(1).ok_or("output directory required")?);
    let filter = env::args().nth(2);
    let frost_case = env::args().nth(3);
    if let Some(ref case) = frost_case
        && (filter.as_deref() != Some("frost")
            || ![
                "black",
                "red",
                "media-blue",
                "media-clear",
                "controls-black",
                "controls-clear",
                "controls-driven",
                "controls-reference",
            ]
            .contains(&case.as_str()))
    {
        return Err("third argument requires frost and a known Frost fixture".into());
    }
    if let Some(ref filter) = filter {
        for id in filter.split(',') {
            if still_scenes::descriptor(id).is_none() {
                return Err(format!("unknown scene: {id}").into());
            }
        }
    }
    fs::create_dir_all(&destination)?;
    let r = Renderer::new()?;
    let device = &r.device;
    device.push_error_scope(wgpu::ErrorFilter::Validation);
    let texture_entry = |binding| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            multisampled: false,
            view_dimension: wgpu::TextureViewDimension::D2,
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
        },
        count: None,
    };
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Visual QA layout"),
        entries: &[
            texture_entry(0),
            texture_entry(1),
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
    let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Visual QA exposure"),
        contents: bytemuck::cast_slice(&[0.0_f32; 4]),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Visual QA compositor"),
        source: wgpu::ShaderSource::Wgsl(
            include_str!("../../still-renderer/src/compositor.wgsl").into(),
        ),
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[&layout],
        push_constant_ranges: &[],
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Visual QA compositor"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: "fullscreen_vertex",
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: "composite_fragment",
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        multiview: None,
    });
    let extent = wgpu::Extent3d {
        width: 1280,
        height: 840,
        depth_or_array_layers: 1,
    };
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Visual QA image"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let target_view = target.create_view(&Default::default());
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Visual QA readback"),
        size: u64::from(extent.width * extent.height * 4),
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    for descriptor in SCENE_DESCRIPTORS {
        if filter
            .as_ref()
            .is_some_and(|ids| !ids.split(',').any(|id| id == descriptor.stable_id))
        {
            continue;
        }
        for preset in still_scenes::preset_descriptors(descriptor.stable_id).unwrap() {
            if frost_case.is_some() && preset.stable_id != "builtin.opal" {
                continue;
            }
            let mut scene = create_scene(
                descriptor.stable_id,
                device,
                &r.adapter,
                Quality::Balanced,
                7,
            )
            .unwrap();
            assert!(scene.select_builtin_preset(preset.stable_id));
            let saved = scene.target_parameters().clone();
            match frost_case.as_deref() {
                Some("black") => {
                    scene.set_parameter("frost.brightness", 0.0);
                }
                Some("red") => {
                    scene.set_parameter("frost.ambient", 0.0);
                    for i in 1..=3 {
                        for (channel, value) in [("red", 1.0), ("green", 0.0), ("blue", 0.0)] {
                            scene.set_parameter(&format!("frost.light{i}.{channel}"), value);
                        }
                    }
                }
                Some("media-blue" | "media-clear") => {
                    scene.set_ambient_input(AmbientFrame::validated([[0.02, 0.08, 1.0]; 3], 0.7));
                }
                Some("controls-black" | "controls-clear") => {
                    scene.set_ambient_smoothing(AmbientSmoothing {
                        color_seconds: 0.0,
                        attack_seconds: 0.0,
                        release_seconds: 0.0,
                        transform_seconds: 0.0,
                    });
                    scene.set_ambient_controls(Some(AmbientControlFrame {
                        lights: [AmbientLightControl {
                            color: [0.2, 0.1, 1.0],
                            gain: 1.5,
                            offset: [0.15, -0.1],
                            scale: [0.7, 1.3],
                            rotation_degrees: 25.0,
                        }; 3],
                        brightness: 0.0,
                    }));
                }
                Some("controls-driven" | "controls-reference") => {
                    scene.set_ambient_smoothing(AmbientSmoothing {
                        color_seconds: 0.0,
                        attack_seconds: 0.0,
                        release_seconds: 0.0,
                        transform_seconds: 0.0,
                    });
                    let light = AmbientLightControl {
                        color: [0.2, 0.1, 1.0],
                        gain: 1.5,
                        offset: [0.15, -0.1],
                        scale: [0.7, 1.3],
                        rotation_degrees: 25.0,
                    };
                    if frost_case.as_deref() == Some("controls-driven") {
                        scene.set_ambient_controls(Some(AmbientControlFrame {
                            lights: [light; 3],
                            brightness: 0.7,
                        }));
                    } else {
                        let mut values = scene.target_parameters().clone();
                        let descriptor = scene.descriptor();
                        values.set_at(descriptor, 0, saved.get_at(0).unwrap() * 0.7);
                        for i in 0..3 {
                            let o = 6 + i * 9;
                            for j in 0..3 {
                                values.set_at(descriptor, o + j, light.color[j]);
                            }
                            values.set_at(
                                descriptor,
                                o + 3,
                                saved.get_at(o + 3).unwrap() * light.gain,
                            );
                            for j in 0..2 {
                                values.set_at(
                                    descriptor,
                                    o + 4 + j,
                                    saved.get_at(o + 4 + j).unwrap() + light.offset[j],
                                );
                                values.set_at(
                                    descriptor,
                                    o + 6 + j,
                                    saved.get_at(o + 6 + j).unwrap() * light.scale[j],
                                );
                            }
                            values.set_at(
                                descriptor,
                                o + 8,
                                saved.get_at(o + 8).unwrap() + light.rotation_degrees,
                            );
                        }
                        scene.restore_content(scene.seed(), values, None);
                    }
                }
                _ => {}
            }
            scene.prepare(device, [extent.width, extent.height]);
            for frame_index in 0..720 {
                if frame_index == 180 && frost_case.as_deref() == Some("media-clear") {
                    scene.set_ambient_input(None);
                }
                if frame_index == 180 && frost_case.as_deref() == Some("controls-clear") {
                    scene.set_ambient_controls(None);
                }
                let mut encoder = r.create_encoder();
                assert!(
                    scene
                        .encode(
                            &r.queue,
                            &mut encoder,
                            FramePlan {
                                animated: false,
                                ticks: 2,
                                fixed_dt: 1.0 / 120.0,
                                interpolation: 0.0,
                                display_dt: 1.0 / 60.0
                            },
                            PointerInput::default()
                        )
                        .is_some()
                );
                r.queue.submit(Some(encoder.finish()));
                device.poll(wgpu::Maintain::Wait);
                if frost_case.is_none() && frame_index == 359 {
                    break;
                }
            }
            if matches!(
                frost_case.as_deref(),
                Some(
                    "media-blue"
                        | "media-clear"
                        | "controls-black"
                        | "controls-clear"
                        | "controls-driven"
                )
            ) {
                assert_eq!(scene.target_parameters(), &saved);
            }
            let mut encoder = r.create_encoder();
            let output = scene
                .encode(
                    &r.queue,
                    &mut encoder,
                    FramePlan::default(),
                    PointerInput::default(),
                )
                .unwrap();
            r.queue.write_buffer(
                &uniform,
                0,
                bytemuck::cast_slice(&[output.exposure, output.exposure, 0.0, 0.0]),
            );
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(output.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(output.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: uniform.as_entire_binding(),
                    },
                ],
            });
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: None,
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &target_view,
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
                pass.set_pipeline(&pipeline);
                pass.set_bind_group(0, &group, &[]);
                pass.draw(0..3, 0..1);
            }
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
                        bytes_per_row: Some(extent.width * 4),
                        rows_per_image: Some(extent.height),
                    },
                },
                extent,
            );
            r.queue.submit(Some(encoder.finish()));
            let slice = readback.slice(..);
            let (tx, rx) = std::sync::mpsc::channel();
            slice.map_async(wgpu::MapMode::Read, move |result| {
                let _ = tx.send(result);
            });
            device.poll(wgpu::Maintain::Wait);
            rx.recv()??;
            let bytes = slice.get_mapped_range();
            let filename = format!("{}-{}.rgba", descriptor.stable_id, preset.stable_id);
            fs::write(destination.join(&filename), &bytes)?;
            println!(
                "{} / {} -> {}",
                descriptor.display_name, preset.display_name, filename
            );
            drop(bytes);
            readback.unmap();
        }
    }
    if let Some(error) = pollster::block_on(device.pop_error_scope()) {
        return Err(error.into());
    }
    Ok(())
}
