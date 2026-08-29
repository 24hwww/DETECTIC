# Detectic — build recipes
#
# Host (development) build: full binary with SQLite persistence.
#   make build
#   make test
#
# Router (TP-Link EX520v, MediaTek MT7981, ARM64, OpenWrt/musl) build:
#   tiny, static, NO C dependencies, persistence disabled (sensor uploads instead).
#   make router           (local, needs aarch64-musl toolchain)
#   make router-docker    (recommended, uses messense/rust-musl-cross)
#   make package          (router binary + flat manifest + deps into deploy/ex520_package)
#   => target/aarch64-unknown-linux-musl/release/detectic
#   => dist/detectic-aarch64-musl
#   => deploy/ex520_package/{detectic.a?,manifest.json,version}

TARGET := aarch64-unknown-linux-musl
RUST_LLD := $(HOME)/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/lib/rustlib/x86_64-unknown-linux-gnu/bin/rust-lld
MUSL_IMAGE := messense/rust-musl-cross:aarch64-musl
PACKAGE_DIR := deploy/ex520_package

.PHONY: build router router-docker package test clean

build:
	cargo build --release

# On-router sensor: pure Rust, static, no SQLite (persist feature off).
# wss = WebSocket transport to Cloudflare Durable Object (realtime dashboard)
# tls = HTTPS support for ureq (fallback upload + spool drain)
router:
	RUSTFLAGS="-C link-self-contained=yes -C linker=$(RUST_LLD)" \
		cargo build --release --target $(TARGET) --no-default-features --features wss,tls

# Recommended cross-build via Docker (no local aarch64 toolchain needed).
# Produces the exact binary that the EX520 must run (NO C deps, wss+tls).
router-docker:
	docker run --rm -v "$(CURDIR):/home/rust/src" $(MUSL_IMAGE) \
		cargo build --release --no-default-features --features wss,tls
	mkdir -p dist
	cp target/$(TARGET)/release/detectic dist/detectic-aarch64-musl
	chmod +x dist/detectic-aarch64-musl
	@echo "Built dist/detectic-aarch64-musl"

# Cross-build the external RF probe sensor for OpenWrt/embedded Linux arm64.
# Static musl, no C deps, HTTPS support.
extsensor-docker:
	docker run --rm -v "$(CURDIR):/home/rust/src" $(MUSL_IMAGE) \
		cargo build --release --bin extsensor --no-default-features --features tls
	mkdir -p dist
	cp target/$(TARGET)/release/extsensor dist/extsensor-aarch64-musl
	chmod +x dist/extsensor-aarch64-musl
	@echo "Built dist/extsensor-aarch64-musl"

# Build the deployable package (split parts + flat manifest + verifiers) into the
# served package dir. Idempotent; safe to re-run.
package: router-docker
	bash $(PACKAGE_DIR)/build_package.sh
	@echo "Package ready in $(PACKAGE_DIR)/ and served by package_server.py"

test:
	cargo test

clean:
	cargo clean
