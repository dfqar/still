use still_renderer::wgpu;

pub(crate) fn output_format(adapter: &wgpu::Adapter) -> wgpu::TextureFormat {
    let format = wgpu::TextureFormat::Rgba16Float;
    let features = adapter.get_texture_format_features(format);
    if features
        .allowed_usages
        .contains(wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING)
        && features
            .flags
            .contains(wgpu::TextureFormatFeatureFlags::FILTERABLE)
    {
        format
    } else {
        wgpu::TextureFormat::Rgba8Unorm
    }
}

pub(crate) fn capped_size(viewport: [u32; 2], megapixels: f32) -> Option<[u32; 2]> {
    if viewport.contains(&0) {
        return None;
    }
    let pixels = f64::from(viewport[0]) * f64::from(viewport[1]);
    let scale = (f64::from(megapixels) * 1_000_000.0 / pixels)
        .sqrt()
        .min(1.0);
    Some(viewport.map(|dimension| (f64::from(dimension) * scale).round().max(1.0) as u32))
}

pub(crate) struct ColorTarget {
    pub _texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub size: [u32; 2],
}

impl ColorTarget {
    pub fn new(
        device: &wgpu::Device,
        size: [u32; 2],
        format: wgpu::TextureFormat,
        label: &str,
    ) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        Self {
            _texture: texture,
            view,
            size,
        }
    }
}
