use anyhow::Result;
use image::GenericImageView;

pub(crate) struct Texture {
  #[allow(unused)]
  pub(crate) texture: wgpu::Texture,
  pub(crate) view: wgpu::TextureView,
  pub(crate) sampler: wgpu::Sampler,
  #[allow(dead_code)]
  pub(crate) label: String,
}

impl Texture {
  #[allow(dead_code)]
  pub(crate) fn from_bytes(device: &wgpu::Device, queue: &wgpu::Queue, bytes: &[u8], format: wgpu::TextureFormat, label: &str) -> Result<Self> {
    let img = image::load_from_memory(bytes)?;
    Self::from_image(device, queue, &img, format, Some(label))
  }

  pub(crate) fn from_file(device: &wgpu::Device, queue: &wgpu::Queue, path: &str) -> Result<Self> {
    let img = image::open(path)?;
    Self::from_image(device, queue, &img, wgpu::TextureFormat::Rgba8UnormSrgb, Some(path))
  }

  pub(crate) fn from_image(device: &wgpu::Device, queue: &wgpu::Queue, img: &image::DynamicImage, format: wgpu::TextureFormat, label: Option<&str>) -> Result<Self> {
    let rgba = img.to_rgba8();
    let dimensions = img.dimensions();

    let size = wgpu::Extent3d {
      width: dimensions.0,
      height: dimensions.1,
      depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
      label,
      size,
      mip_level_count: 1,
      sample_count: 1,
      dimension: wgpu::TextureDimension::D2,
      format,
      usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
      view_formats: &[],
    });

    queue.write_texture(
      wgpu::TexelCopyTextureInfo {
        aspect: wgpu::TextureAspect::All,
        texture: &texture,
        mip_level: 0,
        origin: wgpu::Origin3d::ZERO,
      },
      &rgba,
      wgpu::TexelCopyBufferLayout {
        offset: 0,
        bytes_per_row: Some(4 * dimensions.0),
        rows_per_image: Some(dimensions.1),
      },
      size,
    );

    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
      address_mode_u: wgpu::AddressMode::Repeat,
      address_mode_v: wgpu::AddressMode::Repeat,
      address_mode_w: wgpu::AddressMode::Repeat,
      mag_filter: wgpu::FilterMode::Linear,
      min_filter: wgpu::FilterMode::Nearest,
      mipmap_filter: wgpu::MipmapFilterMode::Nearest,
      ..Default::default()
    });

    Ok(Self {
      texture,
      view,
      sampler,
      label: label.unwrap_or("undefined").to_string(),
    })
  }

  pub(crate) fn new_solid_color(device: &wgpu::Device, queue: &wgpu::Queue, color: [u8; 4], format: wgpu::TextureFormat, label: &str) -> Result<Self> {
    let img = image::ImageBuffer::from_pixel(1, 1, image::Rgba(color));

    Self::from_image(device, queue, &image::DynamicImage::ImageRgba8(img), format, Some(label))
  }

  #[allow(dead_code)]
  pub(crate) fn bind_group_entries(&self) -> [wgpu::BindGroupEntry<'_>; 2] {
    [
      wgpu::BindGroupEntry {
        binding: 0,
        resource: wgpu::BindingResource::TextureView(&self.view),
      },
      wgpu::BindGroupEntry {
        binding: 1,
        resource: wgpu::BindingResource::Sampler(&self.sampler),
      },
    ]
  }
}
