# Detectic — build recipes
#
# Host (development) build: full binary with SQLite persistence.
#   make build
#   make test
#
# Router (TP-Link EX520v, MediaTek MT7981, ARM64, OpenWrt/musl) build:
#   tiny, static, NO C dependencies, persistence disabled (sensor uploads instead).
#   make router
#   => target/aarch64-unknown-linux-musl/release/detectic

TARGET := aarch64-unknown-linux-musl
RUST_LLD := $(HOME)/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/lib/rustlib/x86_64-unknown-linux-gnu/bin/rust-lld

.PHONY: build router test clean

build:
	cargo build --release

# On-router sensor: pure Rust, static, no SQLite (persist feature off).
# wss = WebSocket transport to Cloudflare Durable Object (realtime dashboard)
# tls = HTTPS support for ureq (fallback upload + spool drain)
router:
	RUSTFLAGS="-C link-self-contained=yes -C linker=$(RUST_LLD)" \
		cargo build --release --target $(TARGET) --no-default-features --features wss,tls

test:
	cargo test

clean:
	cargo clean
