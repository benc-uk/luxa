// ============================================================================================
// Generate the neutral HDR equirectangular environment bundled as Luxa's default IBL.
// ============================================================================================

use anyhow::{Context, Result};
use glam::Vec3;
use image::{Rgb, codecs::hdr::HdrEncoder};
use std::{f32::consts, fs::File, path::PathBuf};

const WIDTH: usize = 1024;
const HEIGHT: usize = WIDTH / 2;

fn main() -> Result<()> {
  let output_path = std::env::args_os()
    .nth(1)
    .map(PathBuf::from)
    .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("./assets/default.hdr"));

  let mut pixels = Vec::with_capacity(WIDTH * HEIGHT);
  for row in 0..HEIGHT {
    let latitude = (0.5 - (row as f32 + 0.5) / HEIGHT as f32) * consts::PI;
    let latitude_cos = latitude.cos();

    for column in 0..WIDTH {
      let longitude = ((column as f32 + 0.5) / WIDTH as f32 - 0.5) * consts::TAU;
      let direction = Vec3::new(latitude_cos * longitude.cos(), latitude.sin(), latitude_cos * longitude.sin());
      pixels.push(Rgb(environment_radiance(direction).to_array()));
    }
  }

  let output = File::create(&output_path).with_context(|| format!("failed to create {}", output_path.display()))?;
  HdrEncoder::new(output)
    .encode(&pixels, WIDTH, HEIGHT)
    .with_context(|| format!("failed to encode {}", output_path.display()))?;

  println!("Generated {}x{} default IBL at {}", WIDTH, HEIGHT, output_path.display());
  Ok(())
}

fn environment_radiance(direction: Vec3) -> Vec3 {
  let horizon = Vec3::new(0.46, 0.50, 0.56);
  let zenith = Vec3::new(0.10, 0.17, 0.29);
  let ground = Vec3::new(0.055, 0.052, 0.048);

  let mut radiance = if direction.y >= 0.0 {
    horizon.lerp(zenith, direction.y.sqrt())
  } else {
    ground.lerp(horizon, smoothstep(-0.25, 0.0, direction.y))
  };

  let sun_direction = Vec3::new(0.5, consts::FRAC_1_SQRT_2, -0.5).normalize();
  let sun_angle = direction.dot(sun_direction).clamp(-1.0, 1.0).acos();
  let sun_radius = 5.0_f32.to_radians();
  let sun_disc = 1.0 - smoothstep(sun_radius * 0.75, sun_radius, sun_angle);
  let sun_halo = (-(sun_angle / 0.10).powi(2)).exp();
  let sun_colour = Vec3::new(1.0, 0.82, 0.60);
  radiance += sun_colour * (sun_disc * 30.0 + sun_halo * 0.8);

  radiance
}

fn smoothstep(edge_min: f32, edge_max: f32, value: f32) -> f32 {
  let value = ((value - edge_min) / (edge_max - edge_min)).clamp(0.0, 1.0);
  value * value * (3.0 - 2.0 * value)
}
