.PHONY: run build release build-web release-web check serve fmt clippy clean

run:
	cargo run

run-win:
	cargo run --target x86_64-pc-windows-gnu
	
build:
	cargo build

release:
	cargo build --release

release-win:
	cargo build --release --target x86_64-pc-windows-gnu

check:
	cargo check

serve:
	npx vite . --port 8000

fmt:
	cargo fmt

clippy:
	cargo clippy -- -D warnings

clean:
	cargo clean
	rm -rf pkg