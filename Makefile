.PHONY: dev dev-web check release web install uninstall

dev:
	cargo run -- serve --config config.dev.yaml

dev-web:
	cd web && npm install && npm run dev

web:
	cd web && npm ci && npm run build

check:
	cargo fmt --check
	cargo clippy --all-targets -- -D warnings
	cargo test
	cd web && npm ci && npm test && npm run build
	cd extension && npm test

release: web
	cargo build --release

install: release
	scripts/install.sh target/release/liotek

uninstall:
	scripts/uninstall.sh
