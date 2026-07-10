.PHONY: run build fmt clippy clean shaders release release-windows

run:
	cargo run

run-windows:
	cargo run --target x86_64-pc-windows-gnu
	
build:
	cargo build

release:
	cargo build --release

release-windows:
	cargo build --release --target x86_64-pc-windows-gnu

fmt:
	cargo fmt

clippy:
	cargo clippy

clean:
	cargo clean
