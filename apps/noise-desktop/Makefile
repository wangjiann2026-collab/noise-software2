# NoiseCad Desktop — Build targets
# Run from:  apps/noise-desktop/

APP     = NoiseCad
TAURI   = cargo tauri
BUNDLE  = src-tauri/target/release/bundle

.PHONY: dev build build-arm build-x86 build-universal dmg icons check clean help

## ── Dev server (hot-reload) ─────────────────────────────────────────────
dev:
	$(TAURI) dev

## ── Release build (current arch) ───────────────────────────────────────
build:
	$(TAURI) build

## ── Apple Silicon only ──────────────────────────────────────────────────
build-arm:
	$(TAURI) build --target aarch64-apple-darwin

## ── Intel Mac only ──────────────────────────────────────────────────────
build-x86:
	$(TAURI) build --target x86_64-apple-darwin

## ── Universal binary (arm64 + x86_64) ──────────────────────────────────
build-universal:
	rustup target add aarch64-apple-darwin x86_64-apple-darwin
	$(TAURI) build --target universal-apple-darwin

## ── Open finished .app in Finder ────────────────────────────────────────
open-app:
	open $(BUNDLE)/macos/$(APP).app 2>/dev/null || \
	open $(BUNDLE)/universal-apple-darwin/release/bundle/macos/$(APP).app

## ── DMG from universal build ─────────────────────────────────────────────
dmg: build-universal
	@APP_PATH=$$(find $(BUNDLE) -name "$(APP).dmg" | head -1); \
	if [ -n "$$APP_PATH" ]; then \
		open -R "$$APP_PATH"; \
		echo "DMG: $$APP_PATH"; \
	else \
		echo "DMG not found — check $(BUNDLE)"; \
	fi

## ── Regenerate icons from icon-source.png ───────────────────────────────
icons:
	@if [ -f src-tauri/icons/icon-source.png ]; then \
		$(TAURI) icon src-tauri/icons/icon-source.png; \
	else \
		echo "Missing src-tauri/icons/icon-source.png"; exit 1; \
	fi

## ── Cargo check (fast type-check, no link) ──────────────────────────────
check:
	cd src-tauri && cargo check

## ── Clean build artifacts ────────────────────────────────────────────────
clean:
	cd src-tauri && cargo clean

## ── Help ─────────────────────────────────────────────────────────────────
help:
	@echo ""
	@echo "  make dev              — launch dev window (live reload)"
	@echo "  make build            — release build (current arch)"
	@echo "  make build-universal  — fat binary: arm64 + x86_64"
	@echo "  make dmg              — universal build + open DMG folder"
	@echo "  make icons            — regenerate all icon sizes"
	@echo "  make check            — fast Rust type-check"
	@echo "  make clean            — remove build artifacts"
	@echo ""
