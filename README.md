# Noise Software 2 — 3D Environmental Noise Mapping Platform

A cross-platform 3D environmental noise mapping and acoustic simulation platform built in Rust.

## Supported Platforms
| Platform | Status |
|----------|--------|
| Windows  | Vulkan / DX12 |
| Linux    | Vulkan |
| macOS    | Metal (via wgpu) |
| Web      | WebGPU (WASM) |

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                     CLIENT LAYER                        │
│   CLI (clap)  │  Desktop (Tauri+React)  │  Web (WASM)  │
└───────────────────────────┬─────────────────────────────┘
                            │
┌───────────────────────────▼─────────────────────────────┐
│              API & PROTOCOL LAYER                        │
│         REST API (Axum)  │  MCP Server                  │
└───────────────────────────┬─────────────────────────────┘
                            │
┌───────────────────────────▼─────────────────────────────┐
│              ACOUSTIC CORE ENGINE                        │
│  Ray Tracing  │  Angle Scanning  │  Propagation Models  │
│  ISO 9613-2 / CNOSSOS-EU  │  Rayon parallel scheduler  │
└───────────────────────────┬─────────────────────────────┘
                            │
┌───────────────────────────▼─────────────────────────────┐
│              DATA & RENDERING LAYER                      │
│  SQLite (rusqlite)  │  wgpu (Vulkan/WebGPU)             │
└─────────────────────────────────────────────────────────┘
```

## Workspace Structure

```
crates/
  noise-core      — Acoustic simulation engine (ray tracing, propagation, metrics)
  noise-data      — Data models, SQLite, scenario variants, geometric transforms
  noise-render    — 2D/3D rendering via wgpu (Vulkan → WebGPU)
  noise-io        — DXF / Shapefile / GeoJSON / ASCII / XML import-export
  noise-auth      — Argon2 password hashing + JWT authentication
  noise-mcp       — MCP server (AI agent interface)
apps/
  noise-cli       — `noise` CLI binary (full feature access via command line)
  noise-api       — REST API server (Axum)
```

## Quick Start

```bash
# Build all crates
cargo build --release

# Show system information
./target/release/noise info

# Create a new project
./target/release/noise project new --name "City Centre Assessment" --crs 32650

# Run a calculation (Phase 4+)
./target/release/noise calc --project project.nsp --metric Lden --resolution 10

# Start the API + MCP server (Phase 6+)
./target/release/noise server --port 8080 --mcp-port 8081
```

## Development Phases

| Phase | Description | Status |
|-------|-------------|--------|
| 1 | Architecture & scaffold | **Done** |
| 2 | CLI & build infrastructure | **Done** |
| 3 | Database & object management | Next |
| 4 | Acoustic engine (TDD) | Pending |
| 5 | 2D/3D rendering | Pending |
| 6 | I/O parsers & MCP server | Pending |
| 7 | Tauri desktop + Web WASM | Pending |
| 8 | Authentication | Pending |
| 9 | Performance (SIMD + GPU) | Pending |

## Key Acoustic Features
- Ray tracing with up to **20th-order reflections** (image source method)
- Angle scanning for distributed sources (road, railway)
- Propagation models: **ISO 9613-2**, **CNOSSOS-EU**
- Source types: road traffic, railway, point source, line source
- Metrics: **Ld, Le, Ln, Lden, Ldn, L10, L1hmax**, user-defined formulas
- Grids: horizontal, vertical cross-section, building façade
- Multi-scenario variant comparison

## License
MIT OR Apache-2.0
