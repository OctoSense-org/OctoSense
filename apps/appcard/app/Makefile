# octos-app — local dev loop.
#
# Mirrors the CI lane (.github/workflows/ci.yml) plus a few binary-only
# commands (run, smoke-live) that CI doesn't cover because the binary is
# excluded from CI under W10's option (c). Source `.env` automatically if
# present so `OCTOS_APP_TOKEN` etc. flow into `make run` / `make smoke-live`.

ifneq (,$(wildcard .env))
include .env
export
endif

# Live-smoke defaults; override per-shell with `make smoke-live OCTOS_LIVE_URL=...`.
OCTOS_LIVE_URL ?= http://127.0.0.1:56831

.PHONY: help check build test run smoke-live fmt clippy clean

help:
	@echo "octos-app dev targets"
	@echo "  make check       cargo check --workspace"
	@echo "  make build       cargo build --workspace"
	@echo "  make test        cargo test --workspace"
	@echo "  make run         cargo run -p octos-app (reads OCTOS_APP_TOKEN from env / .env)"
	@echo "  make smoke-live  run the #[ignore]-gated live_smoke test against OCTOS_LIVE_URL"
	@echo "  make fmt         cargo fmt --all"
	@echo "  make clippy      cargo clippy --workspace --all-targets"
	@echo "  make clean       cargo clean"
	@echo "  make help        this message"

check:
	cargo check --workspace

build:
	cargo build --workspace

test:
	cargo test --workspace

run:
	cargo run -p octos-app

smoke-live:
	OCTOS_LIVE_URL="$(OCTOS_LIVE_URL)" \
	cargo test -p octos-app-transport --test live_smoke -- --ignored --nocapture

fmt:
	# Per-package: `--all` would walk into `../aichat` path-deps where
	# the upstream Makepad fork has its own formatting policy.
	cargo fmt -p octos-app -p octos-app-store -p octos-app-transport -p octos-app-render

clippy:
	cargo clippy --workspace --all-targets

clean:
	cargo clean
