// ======================================================================================
// Load glTF scenes into the engine's mesh, material and node resource types.
// See ../PARSER.md for the loading flow, terminology and supported features.
// ======================================================================================

use std::collections::HashMap;

use anyhow::{Context, Result, bail};
use glam::{Mat3, Mat4, Quat, Vec3};
use image::{DynamicImage, ImageBuffer, Luma, LumaA, Rgb, Rgba};

use crate::{
  engine::{Engine, Node3DHandle, TextureHandle},
  models::{Mesh, Vertex},
};

struct ParsedGltf {
  materials: Vec<ParsedMaterial>,
  primitives: Vec<ParsedPrimitive>,
  images: Vec<DynamicImage>,
}

#[derive(Debug)]
struct ParsedMaterial {
  base_color_factor: [f32; 4],
  base_color_texture: Option<ParsedTexture>,
  metallic_factor: f32,
  roughness_factor: f32,
  metallic_roughness_texture: Option<ParsedTexture>,
  normal_texture: Option<ParsedTexture>,
  normal_scale: f32,
  occlusion_texture: Option<ParsedTexture>,
  occlusion_strength: f32,
  emissive_factor: [f32; 3],
  emissive_texture: Option<ParsedTexture>,
  alpha_mode: gltf::material::AlphaMode,
  alpha_cutoff: Option<f32>,
  double_sided: bool,
}

#[derive(Clone, Copy, Debug)]
struct ParsedTexture {
  image_index: usize,
  tex_coord: u32,
}

struct ParsedPrimitive {
  vertices: Vec<Vertex>,
  indices: Vec<u16>,
  material_index: Option<usize>,
}

impl Engine {
  /// Loads a `.gltf` or `.glb` file and attaches its flattened geometry below `parent`.
  pub fn load_gltf(&mut self, path: &str, parent: Node3DHandle) -> Result<Node3DHandle> {
    let (document, buffers, images) = gltf::import(path).with_context(|| format!("failed to import glTF file {path}"))?;
    let parsed = parse_document(&document, &buffers, images)?;

    self.add_parsed_gltf(parsed, parent)
  }

  /// Loads a self-contained GLB or glTF byte slice and attaches its flattened geometry below `parent`.
  pub fn load_gltf_bytes(&mut self, bytes: &[u8], parent: Node3DHandle) -> Result<Node3DHandle> {
    let (document, buffers, images) = gltf::import_slice(bytes).context("failed to import glTF bytes")?;
    let parsed = parse_document(&document, &buffers, images)?;

    self.add_parsed_gltf(parsed, parent)
  }

  fn add_parsed_gltf(&mut self, parsed: ParsedGltf, parent: Node3DHandle) -> Result<Node3DHandle> {
    let ParsedGltf { materials, primitives, images } = parsed;
    let mut material_handles = Vec::with_capacity(materials.len());

    // A single glTF image can be referenced by many materials (and by several slots within one
    // material). Cache the uploaded texture per (image index, format) so we only push each unique
    // image to the GPU once. Format is part of the key because the same image may be needed as both
    // sRGB (colour) and linear Unorm (data).
    let mut texture_cache: HashMap<(usize, wgpu::TextureFormat), TextureHandle> = HashMap::new();

    for parsed_mat in materials {
      let out_mat = self.create_material(None);

      if let Some(base_color_texture) = parsed_mat.base_color_texture {
        let texture_handle = load_material_texture(self, &images, &mut texture_cache, base_color_texture, wgpu::TextureFormat::Rgba8UnormSrgb)?;
        self.material_mut(out_mat).set_base_color_texture(texture_handle);
      }

      if let Some(metallic_roughness_texture) = parsed_mat.metallic_roughness_texture {
        let texture_handle = load_material_texture(self, &images, &mut texture_cache, metallic_roughness_texture, wgpu::TextureFormat::Rgba8Unorm)?;
        self.material_mut(out_mat).set_metallic_roughness_texture(texture_handle);
      }

      if let Some(emissive_texture) = parsed_mat.emissive_texture {
        let texture_handle = load_material_texture(self, &images, &mut texture_cache, emissive_texture, wgpu::TextureFormat::Rgba8UnormSrgb)?;
        self.material_mut(out_mat).set_emissive_texture(texture_handle);
      }

      if let Some(normal_texture) = parsed_mat.normal_texture {
        let texture_handle = load_material_texture(self, &images, &mut texture_cache, normal_texture, wgpu::TextureFormat::Rgba8Unorm)?;
        self.material_mut(out_mat).set_normal_texture(texture_handle);
      }

      if let Some(occlusion_texture) = parsed_mat.occlusion_texture {
        let texture_handle = load_material_texture(self, &images, &mut texture_cache, occlusion_texture, wgpu::TextureFormat::Rgba8Unorm)?;
        self.material_mut(out_mat).set_occlusion_texture(texture_handle);
      }

      self.material_mut(out_mat).set_base_color_factor(parsed_mat.base_color_factor);
      self.material_mut(out_mat).set_metallic_factor(parsed_mat.metallic_factor);
      self.material_mut(out_mat).set_roughness_factor(parsed_mat.roughness_factor);
      self.material_mut(out_mat).set_normal_scale(parsed_mat.normal_scale);
      self.material_mut(out_mat).set_occlusion_strength(parsed_mat.occlusion_strength);
      self.material_mut(out_mat).set_emissive_factor(parsed_mat.emissive_factor);

      self.material_mut(out_mat).set_alpha_mode(match parsed_mat.alpha_mode {
        gltf::material::AlphaMode::Opaque => crate::models::AlphaMode::Opaque,
        gltf::material::AlphaMode::Mask => crate::models::AlphaMode::Mask,
        gltf::material::AlphaMode::Blend => crate::models::AlphaMode::Blend,
      });
      log::debug!("  Material   alpha mode: {:?}", parsed_mat.alpha_mode);

      if let Some(cutoff) = parsed_mat.alpha_cutoff {
        self.material_mut(out_mat).set_alpha_cutoff(cutoff);
      }

      self.material_mut(out_mat).set_double_sided(parsed_mat.double_sided);

      material_handles.push(out_mat);
    }

    let mut mesh_handles = Vec::with_capacity(primitives.len());
    for primitive in primitives {
      let material = primitive.material_index.map_or_else(|| self.default_material(), |index| material_handles[index]);
      let mesh = Mesh::new(self, primitive.vertices, primitive.indices, material);
      mesh_handles.push(self.add_mesh(mesh));
    }

    Ok(self.create_mesh_node(parent, mesh_handles, Vec3::ZERO, Quat::IDENTITY, Vec3::ONE))
  }
}

fn parse_document(document: &gltf::Document, buffers: &[gltf::buffer::Data], images: Vec<gltf::image::Data>) -> Result<ParsedGltf> {
  log::info!(
    "Parsing glTF document with {} scenes, {} materials, and {} images",
    document.scenes().len(),
    document.materials().len(),
    images.len()
  );

  let scene = document.default_scene().or_else(|| document.scenes().next()).context("glTF document contains no scenes")?;

  let materials = document
    .materials()
    .map(|material| {
      log::debug!("Parsing material {}: {}", material.index().unwrap(), material.name().unwrap());
      let pbr = material.pbr_metallic_roughness();

      let parsed_mat = ParsedMaterial {
        base_color_factor: pbr.base_color_factor(),
        base_color_texture: pbr.base_color_texture().map(|info| ParsedTexture {
          image_index: info.texture().source().index(),
          tex_coord: info.tex_coord(),
        }),
        metallic_factor: pbr.metallic_factor(),
        roughness_factor: pbr.roughness_factor(),
        metallic_roughness_texture: pbr.metallic_roughness_texture().map(|info| ParsedTexture {
          image_index: info.texture().source().index(),
          tex_coord: info.tex_coord(),
        }),
        normal_texture: material.normal_texture().map(|info| ParsedTexture {
          image_index: info.texture().source().index(),
          tex_coord: info.tex_coord(),
        }),
        normal_scale: material.normal_texture().map_or(1.0, |info| info.scale()),
        occlusion_texture: material.occlusion_texture().map(|info| ParsedTexture {
          image_index: info.texture().source().index(),
          tex_coord: info.tex_coord(),
        }),
        occlusion_strength: material.occlusion_texture().map_or(1.0, |info| info.strength()),
        emissive_factor: material.emissive_factor(),
        emissive_texture: material.emissive_texture().map(|info| ParsedTexture {
          image_index: info.texture().source().index(),
          tex_coord: info.tex_coord(),
        }),
        alpha_mode: material.alpha_mode(),
        alpha_cutoff: material.alpha_cutoff(),
        double_sided: material.double_sided(),
      };
      log::debug!("  {:?}", parsed_mat);
      parsed_mat
    })
    .collect();

  let mut primitives = Vec::new();

  for node in scene.nodes() {
    parse_node(node, Mat4::IDENTITY, buffers, &mut primitives)?;
  }

  if primitives.is_empty() {
    bail!("glTF scene contains no mesh primitives");
  }

  let images = images.into_iter().map(gltf_image_to_dynamic_image).collect::<Result<Vec<_>>>()?;

  Ok(ParsedGltf { materials, primitives, images })
}

fn load_material_texture(
  engine: &mut Engine,
  images: &[DynamicImage],
  cache: &mut HashMap<(usize, wgpu::TextureFormat), TextureHandle>,
  texture: ParsedTexture,
  format: wgpu::TextureFormat,
) -> Result<TextureHandle> {
  if texture.tex_coord != 0 {
    log::warn!("glTF texture uses TEXCOORD_{}; the engine currently samples TEXCOORD_0", texture.tex_coord);
  }

  // Check the cache first, because the same glTF image may be referenced by multiple materials (and by multiple slots within one material).
  if let Some(&handle) = cache.get(&(texture.image_index, format)) {
    return Ok(handle);
  }

  let image = images
    .get(texture.image_index)
    .with_context(|| format!("material references missing glTF image {}", texture.image_index))?;

  log::debug!("  Creating texture for glTF image {} with format {:?}", texture.image_index, format);

  let handle = engine.create_texture_from_image(image, format, &format!("glTF image {}", texture.image_index))?;
  cache.insert((texture.image_index, format), handle);

  Ok(handle)
}

fn gltf_image_to_dynamic_image(data: gltf::image::Data) -> Result<DynamicImage> {
  let gltf::image::Data { pixels, format, width, height } = data;

  let invalid_data = || anyhow::anyhow!("invalid pixel data for glTF image {width}x{height} with format {format:?}");

  Ok(match format {
    gltf::image::Format::R8 => DynamicImage::ImageLuma8(ImageBuffer::<Luma<u8>, _>::from_raw(width, height, pixels).ok_or_else(invalid_data)?),
    gltf::image::Format::R8G8 => DynamicImage::ImageLumaA8(ImageBuffer::<LumaA<u8>, _>::from_raw(width, height, pixels).ok_or_else(invalid_data)?),
    gltf::image::Format::R8G8B8 => DynamicImage::ImageRgb8(ImageBuffer::<Rgb<u8>, _>::from_raw(width, height, pixels).ok_or_else(invalid_data)?),
    gltf::image::Format::R8G8B8A8 => DynamicImage::ImageRgba8(ImageBuffer::<Rgba<u8>, _>::from_raw(width, height, pixels).ok_or_else(invalid_data)?),
    gltf::image::Format::R16 => DynamicImage::ImageLuma16(ImageBuffer::<Luma<u16>, _>::from_raw(width, height, bytes_to_u16(pixels)?).ok_or_else(invalid_data)?),
    gltf::image::Format::R16G16 => DynamicImage::ImageLumaA16(ImageBuffer::<LumaA<u16>, _>::from_raw(width, height, bytes_to_u16(pixels)?).ok_or_else(invalid_data)?),
    gltf::image::Format::R16G16B16 => DynamicImage::ImageRgb16(ImageBuffer::<Rgb<u16>, _>::from_raw(width, height, bytes_to_u16(pixels)?).ok_or_else(invalid_data)?),
    gltf::image::Format::R16G16B16A16 => DynamicImage::ImageRgba16(ImageBuffer::<Rgba<u16>, _>::from_raw(width, height, bytes_to_u16(pixels)?).ok_or_else(invalid_data)?),
    gltf::image::Format::R32G32B32FLOAT => DynamicImage::ImageRgb32F(ImageBuffer::<Rgb<f32>, _>::from_raw(width, height, bytes_to_f32(pixels)?).ok_or_else(invalid_data)?),
    gltf::image::Format::R32G32B32A32FLOAT => DynamicImage::ImageRgba32F(ImageBuffer::<Rgba<f32>, _>::from_raw(width, height, bytes_to_f32(pixels)?).ok_or_else(invalid_data)?),
  })
}

fn bytes_to_u16(bytes: Vec<u8>) -> Result<Vec<u16>> {
  if !bytes.len().is_multiple_of(2) {
    bail!("invalid 16-bit glTF image byte count {}", bytes.len());
  }

  Ok(bytes.chunks_exact(2).map(|bytes| u16::from_ne_bytes([bytes[0], bytes[1]])).collect())
}

fn bytes_to_f32(bytes: Vec<u8>) -> Result<Vec<f32>> {
  if !bytes.len().is_multiple_of(4) {
    bail!("invalid 32-bit glTF image byte count {}", bytes.len());
  }

  Ok(bytes.chunks_exact(4).map(|bytes| f32::from_ne_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])).collect())
}

fn parse_node(node: gltf::Node<'_>, parent_transform: Mat4, buffers: &[gltf::buffer::Data], primitives: &mut Vec<ParsedPrimitive>) -> Result<()> {
  let local_transform = Mat4::from_cols_array_2d(&node.transform().matrix());
  let world_transform = parent_transform * local_transform;

  if let Some(mesh) = node.mesh() {
    for primitive in mesh.primitives() {
      let mesh_index = mesh.index();
      let primitive_index = primitive.index();
      let parsed = parse_primitive(primitive, world_transform, buffers).with_context(|| format!("failed to parse mesh {mesh_index} primitive {primitive_index}"))?;
      primitives.push(parsed);
    }
  }

  for child in node.children() {
    parse_node(child, world_transform, buffers, primitives)?;
  }

  Ok(())
}

fn parse_primitive(primitive: gltf::Primitive<'_>, transform: Mat4, buffers: &[gltf::buffer::Data]) -> Result<ParsedPrimitive> {
  if primitive.mode() != gltf::mesh::Mode::Triangles {
    bail!("unsupported primitive mode {:?}; only triangle lists are supported", primitive.mode());
  }

  log::debug!(
    "Parsing primitive {} with material: {}",
    primitive.index(),
    primitive.material().name().unwrap_or("<unnamed>")
  );

  let reader = primitive.reader(|buffer| buffers.get(buffer.index()).map(|data| data.0.as_slice()));
  let positions = reader
    .read_positions()
    .context("primitive has no POSITION attribute")?
    .map(|position| transform.transform_point3(position.into()))
    .collect::<Vec<Vec3>>();

  if positions.len() > usize::from(u16::MAX) + 1 {
    bail!("primitive has {} vertices; the engine supports at most {}", positions.len(), usize::from(u16::MAX) + 1);
  }

  let mut indices = match reader.read_indices() {
    Some(indices) => indices
      .into_u32()
      .map(|index| u16::try_from(index).with_context(|| format!("index {index} exceeds the engine's u16 index limit")))
      .collect::<Result<Vec<_>>>()?,
    None => (0..positions.len()).map(|index| u16::try_from(index).expect("vertex count was checked above")).collect(),
  };

  if indices.len() % 3 != 0 {
    bail!("triangle-list primitive has {} indices, which is not divisible by 3", indices.len());
  }
  if let Some(index) = indices.iter().copied().find(|index| usize::from(*index) >= positions.len()) {
    bail!("index {index} is outside the primitive's {} vertices", positions.len());
  }

  let reflected = Mat3::from_mat4(transform).determinant() < 0.0;
  if reflected {
    for triangle in indices.chunks_exact_mut(3) {
      triangle.swap(1, 2);
    }
  }

  let normals = match reader.read_normals() {
    Some(normals) => {
      let linear_transform = Mat3::from_mat4(transform);
      if linear_transform.determinant().abs() <= f32::EPSILON {
        bail!("cannot transform normals through a non-invertible node transform");
      }
      let normal_transform = linear_transform.inverse().transpose();
      let normals = normals.map(|normal| (normal_transform * Vec3::from(normal)).normalize_or_zero()).collect::<Vec<_>>();
      if normals.len() != positions.len() {
        bail!("NORMAL attribute has {} values but POSITION has {}", normals.len(), positions.len());
      }
      normals
    }
    None => generate_normals(&positions, &indices),
  };

  let tex_coords = match reader.read_tex_coords(0) {
    Some(tex_coords) => {
      let tex_coords = tex_coords.into_f32().collect::<Vec<_>>();
      if tex_coords.len() != positions.len() {
        bail!("TEXCOORD_0 attribute has {} values but POSITION has {}", tex_coords.len(), positions.len());
      }
      tex_coords
    }
    None => vec![[0.0, 0.0]; positions.len()],
  };

  let vertices = positions
    .into_iter()
    .zip(normals)
    .zip(tex_coords)
    .map(|((position, normal), tex_coord)| Vertex {
      position: position.to_array(),
      tex_coord,
      normal: normal.to_array(),
    })
    .collect();

  Ok(ParsedPrimitive {
    vertices,
    indices,
    material_index: primitive.material().index(),
  })
}

fn generate_normals(positions: &[Vec3], indices: &[u16]) -> Vec<Vec3> {
  let mut normals = vec![Vec3::ZERO; positions.len()];

  for triangle in indices.chunks_exact(3) {
    let first = usize::from(triangle[0]);
    let second = usize::from(triangle[1]);
    let third = usize::from(triangle[2]);
    let face_normal = (positions[second] - positions[first]).cross(positions[third] - positions[first]);
    normals[first] += face_normal;
    normals[second] += face_normal;
    normals[third] += face_normal;
  }

  for normal in &mut normals {
    *normal = normal.normalize_or_zero();
  }

  normals
}
