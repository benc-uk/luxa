.PHONY: build release check serve fmt clippy clean

build:
	wasm-pack build --target web --dev

release:
	wasm-pack build --target web --release

check:
	cargo check --target wasm32-unknown-unknown

serve:
	npx vite . --port 8000

bundle: release
	npx vite build

fmt:
	cargo fmt

clippy:
	cargo clippy --target wasm32-unknown-unknown -- -D warnings

clean:
	cargo clean
	rm -rf pkg
