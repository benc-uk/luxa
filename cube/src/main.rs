#[cfg(not(target_arch = "wasm32"))]
fn main() -> anyhow::Result<()> {
  cube::start()
}

#[cfg(target_arch = "wasm32")]
fn main() {}
