.PHONY: run build release build-web release-web check serve fmt clippy clean

run:
	cargo run

build:
	cargo build

release:
	cargo build --release

build-web:
	wasm-pack build --target web --dev

release-web:
	wasm-pack build --target web --release

check:
	cargo check
	cargo check --target wasm32-unknown-unknown

serve:
	npx vite . --port 8000

fmt:
	cargo fmt

clippy:
	cargo clippy -- -D warnings
	cargo clippy --target wasm32-unknown-unknown -- -D warnings

clean:
	cargo clean
	rm -rf pkg