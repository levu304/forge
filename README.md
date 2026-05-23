# Forge

**A cross-platform 2D CAD viewport with ECS-backed geometry, command-driven interaction, and a 60fps wgpu render pipeline.**

[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](#license)
[![Rust](https://img.shields.io/badge/rust-1.87+-orange)](https://www.rust-lang.org)
[![Status](https://img.shields.io/badge/status-v0.1.0--dev-yellow)](https://github.com/levu/forge)

Forge is a from-scratch CAD engine written in Rust. It combines an ECS architecture for geometric data, a wgpu-based GPU renderer, a nom command parser, and an egui immediate-mode UI to deliver a responsive drafting viewport.

---

## Features

- **Native cross-platform window** -- runs on Linux (X11/Wayland), macOS, and Windows via winit 0.30.
- **wgpu 2D render pipeline** -- orthographic camera, adaptive grid, and per-entity geometry rendering.
- **ECS data model** -- entities and components stored in hecs, queried per frame for batching.
- **Command-driven interaction** -- type commands at the command line or supply inline arguments.
- **Interactive viewport** -- middle-drag to pan, scroll to zoom (zoom toward mouse pointer).
- **Grid overlay** -- major/minor grid lines with axis markers, auto-regenerated on camera change.
- **UI chrome** -- status bar (coordinates, zoom level), command line, toolbar placeholders, property inspector stub.

### Primitives

| Primitive | Status |
|-----------|--------|
| `LINE` | Implemented and tested |
| `CIRCLE` | Parser complete; command stub (v0.2.0) |
| `ARC` | Parser complete; command stub (v0.2.0) |
| `PLINE` | Parser complete; command stub (v0.2.0) |

---

## Prerequisites

- **Rust 1.87+** -- the minimum supported Rust version (MSRV). Install via [rustup](https://rustup.rs/).
- **GPU drivers** compatible with Vulkan, Metal, or DirectX 12, depending on your platform.

### Platform-specific notes

| Platform | Requirements |
|----------|--------------|
| **Linux** | Vulkan drivers (or Mesa software renderer). Wayland is supported via winit 0.30. |
| **macOS** | macOS 10.13+ required. wgpu uses the Metal backend. |
| **Windows** | Windows 10+ recommended. wgpu uses DirectX 12 or Vulkan. |

---

## Build & Run

```sh
# Clone the repository
git clone <repository-url>
cd forge

# Build (debug)
cargo build

# Build (release, optimised for 60fps)
cargo build --release

# Run
cargo run --release
```

The application opens a 1280x720 window titled "Forge v0.1.0".

### Build profiles

The `Cargo.toml` includes release-profile optimisation:

```toml
[profile.release]
opt-level = 3
lto = "thin"
codegen-units = 1
strip = true
```

---

## Usage

### Viewport controls

| Action | Input |
|--------|-------|
| Pan | Middle mouse button + drag |
| Zoom | Scroll wheel (zooms toward mouse pointer) |
| Cancel command | Escape |

### Command line

The command line panel sits at the bottom of the window, above the status bar. Type commands and press Enter to execute them.

**Supported commands:**

```
LINE 0,0 100,100          Draw a single segment from (0,0) to (100,100)
LINE 0,0 100,0 100,100    Accumulate points, Enter to commit (3 segments)
LINE                      Start interactive mode, pick points with clicks
L 0,0 100,100             Short alias for LINE
U                         Undo the last point during interactive LINE
```

**Commands planned for v0.2.0:**

```
CIRCLE 50,50 25           Circle with centre (50,50) and radius 25
ARC 0,0 50 0 90           Arc at centre (0,0), radius 50, 0 to 90 degrees
PLINE 0,0 100,0 100,100 C  Closed polyline (rectangle) with close token
```

### Coordinate format

Coordinates are typed as `x,y` with optional spaces after the comma:

```text
LINE 10.5, 20.75 100,200
LINE -10,-20 30,-40
CIRCLE 1e2,2e2 5e-1         Scientific notation
```

---

## Architecture

```
┌────────────────────────────────────────────────────────────┐
│                    APPLICATION LAYER                       │
│  ┌─────────────┐  ┌─────────────┐  ┌───────────────────┐  │
│  │  App State  │  │   Command   │  │   UI Chrome       │  │
│  │  (ECS)      │  │   System    │  │  (egui)           │  │
│  └──────┬──────┘  └──────┬──────┘  └─────────┬─────────┘  │
│         └────────────────┼───────────────────┘             │
│                          │                                 │
├──────────────────────────┼─────────────────────────────────┤
│                          │                                 │
│  ┌───────────────────────▼─────────────────────────────┐  │
│  │              RENDER LAYER (wgpu)                    │  │
│  │  ┌─────────────┐  ┌─────────────┐  ┌────────────┐  │  │
│  │  │   Camera    │  │   Grid      │  │  Geometry  │  │  │
│  │  │  (Ortho)    │  │  Renderer   │  │  Renderer  │  │  │
│  │  └─────────────┘  └─────────────┘  └────────────┘  │  │
│  │  ┌────────────────────────────────────────────────┐  │  │
│  │  │         Render Pass Orchestrator               │  │  │
│  │  │  (clear -> grid -> entities -> UI overlay)     │  │  │
│  │  └────────────────────────────────────────────────┘  │  │
│  └───────────────────────────────────────────────────────┘  │
│                          │                                 │
├──────────────────────────┼─────────────────────────────────┤
│                          │                                 │
│  ┌───────────────────────▼─────────────────────────────┐  │
│  │              PLATFORM LAYER (winit)                 │  │
│  │  ┌─────────────┐  ┌─────────────┐  ┌────────────┐  │  │
│  │  │   Window    │  │   Events    │  │   Input    │  │  │
│  │  │  Creation   │  │   Loop      │  │   Mapping  │  │  │
│  │  └─────────────┘  └─────────────┘  └────────────┘  │  │
│  └───────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

### Layers

1. **Application Layer** -- `ForgeApp` owns the ECS world (`hecs`), command state, UI system (`egui`), and input mapper. It orchestrates the per-frame render pipeline and command dispatch.

2. **Render Layer** -- wgpu-based 2D pipeline with four passes per frame: clear -> grid -> entities -> UI overlay. The grid and entity renderers share a camera uniform buffer at bind group 0.

3. **Platform Layer** -- winit 0.30 provides window creation, event loop (`ApplicationHandler` trait), and raw input events. The input mapper translates these into application-level actions.

### Key crates

| Crate | Role |
|-------|------|
| `winit` 0.30 | Windowing and event loop |
| `wgpu` 29 | GPU abstraction (Vulkan/Metal/DX12) |
| `hecs` 0.10 | ECS framework for geometry storage |
| `nalgebra` 0.34 | Linear algebra (view-projection matrices) |
| `nom` 8 | Command-line parser combinators |
| `egui` 0.34 | Immediate-mode UI panels |
| `egui-wgpu` 0.34 | egui-to-wgpu render pass bridge |
| `egui-winit` 0.34 | egui-to-winit event translation |
| `thiserror` 2 | Error type definitions |
| `tracing` 0.1 | Structured logging |
| `pollster` 0.4 | Async executor for GPU initialisation |
| `bytemuck` 1.25 | Safe type casting for GPU buffer uploads |

### Module structure

```
src/
├── main.rs              Entry point, winit ApplicationHandler
├── lib.rs               Public API re-exports
├── app.rs               ForgeApp: owns World, RenderState, CommandState
├── ecs/                 ECS components and resources
│   ├── components.rs    LineData, CircleData, ArcData, PolylineData, Renderable
│   └── resources.rs     CameraState, GridConfig, InputState
├── geometry/            Math and geometric types
│   ├── point.rs         Point2D (f64 precision)
│   ├── bounds.rs        BoundingBox2D with union operations
│   ├── vector.rs        Vector2D (reserved for v0.2.0)
│   └── transform.rs     Transform2D (reserved for v0.2.0)
├── commands/            Command system
│   ├── mod.rs           Command trait, CommandState machine
│   ├── parser.rs        nom v8 grammar (LINE, CIRCLE, ARC, PLINE)
│   ├── line_cmd.rs      LINE implementation (buffer-then-commit)
│   ├── circle_cmd.rs    Stub (v0.2.0)
│   ├── arc_cmd.rs       Stub (v0.2.0)
│   └── polyline_cmd.rs  Stub (v0.2.0)
├── render/              wgpu render pipeline
│   ├── mod.rs           RenderState, EntityRenderer
│   ├── camera.rs        OrthographicCamera, compute_view_proj_matrix
│   ├── grid.rs          GridRenderer (vertex-buffered line list)
│   ├── entity_renderer.rs  Per-type batching and tessellation
│   ├── pipeline.rs      Reserved (v0.2.0+)
│   └── shaders/         WGSL shaders (grid.wgsl, entity.wgsl)
├── ui/                  egui panels
│   ├── mod.rs           UiSystem, UiOutput
│   ├── status_bar.rs    Cursor coordinates, zoom, version
│   ├── command_line.rs  Text input with history and error display
│   ├── toolbar.rs       Icon button placeholders (v0.2.0+)
│   └── property_panel.rs  Property inspector stub (v0.2.0+)
├── input/               Input abstraction
│   ├── mod.rs           InputMapper: winit events -> InputActions
│   ├── camera_control.rs  Pan and zoom camera manipulation
│   └── command_input.rs Reserved
└── util/                Utilities
    ├── color.rs         RGBA Color (f32, Pod/Zeroable)
    └── error.rs         ForgeError enum
```

---

## Rendering pipeline

Each frame executes these passes in order:

1. **Clear** -- fill the framebuffer with `CameraState.clear_color` (dark grey `#2B2B2B`).
2. **Grid** -- draw major/minor grid lines and X/Y axes as a single `LineList` draw call. Regenerated when the camera zoom or position changes by 10% or more.
3. **Entities** -- all ECS entities with a `Renderable` component, batched by type into per-type staging buffers. Lines use `LineList`; circles and arcs use `LineStrip` (adaptive tessellation based on screen-space radius).
4. **UI overlay** -- egui panels (status bar, command line, toolbar, property panel).

All shaders (grid and entity) share a single camera uniform at `@group(0) @binding(0)`.

---

## Command system

Commands follow a state-machine design with a `Command` trait:

```
CommandInput::Point(p)     ->  CommandResult::Continue
                          ->  CommandResult::Complete
                          ->  CommandResult::Error(msg)
CommandInput::Cancel       ->  CommandResult::Cancelled
CommandInput::Confirm      ->  (commit to ECS)
```

The `LINE` command uses a **buffer-then-commit** pattern: points accumulate in memory and are spawned into the ECS only on confirmation. Cancelling simply drops the buffer -- no entities to clean up.

The nom-based parser supports all four command types (`LINE`, `CIRCLE`, `ARC`, `PLINE`) with single-letter aliases (`L`, `C`, `A`, `PL`) and partial argument specification. Commands can be invoked with inline arguments or in interactive mode.

---

## Performance targets (v0.1.0)

| Metric | Target |
|--------|--------|
| Viewport FPS | >= 60 fps (<= 16.6 ms frame time) |
| Entity count | 10,000 lines at 60 fps (stress test) |
| Startup time | < 2 seconds |
| Command dispatch | < 1 ms (Enter -> entity spawn) |
| Grid regeneration | < 5 ms |
| Camera pan/zoom | < 1 ms (no frame drops) |

Performance measured on Intel Iris Xe / Apple M1 / NVIDIA GTX 1650 or equivalent integrated GPU.

---

## Development

### Running tests

```sh
# Run all unit tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run specific test module
cargo test commands::parser
```

### Code structure

The project is both a binary and a library crate. The `lib.rs` re-exports all modules for integration testing:

```rust
pub mod app;
pub mod ecs;
pub mod geometry;
pub mod commands;
pub mod render;
pub mod ui;
pub mod input;
pub mod util;
```

### Logging

Structured logging uses the `tracing` crate. Set the `RUST_LOG` environment variable to control verbosity:

```sh
RUST_LOG=forge=debug cargo run
RUST_LOG=forge=trace cargo run   # Very verbose
```

Default filter: `forge=info,wgpu=warn`.

---

## Roadmap

High-level development plan:

| Version | Focus |
|---------|-------|
| **v0.1.0** (current) | Viewport foundation: window, render pipeline, ECS, LINE command, grid, UI chrome |
| **v0.2.0** | Selection system, snapping engine, modify commands (MOVE, COPY, ERASE), undo/redo |
| **v0.3.0** | Layer system, block system, property management, advanced draw commands |
| **v0.4.0** | Dimensioning, text/annotations, hatch patterns |
| **v0.5.0** | DXF I/O, native `.forge` format, SVG/PDF/PNG export |
| **v0.6.0** | Advanced geometry: trim/extend, fillet/chamfer, boolean operations |
| **v0.7.0** | Parametric design: geometric and dimensional constraints, expressions |
| **v0.8.0** | 3D viewing: perspective camera, orbit navigation, mesh display |
| **v1.0.0** | Stable release: performance audit, accessibility, documentation |

See [ROADMAP.md](.docs/ROADMAP.md) for the full roadmap.

---

## License

Licensed under either of:

- MIT License ([LICENSE-MIT](LICENSE-MIT) or https://opensource.org/licenses/MIT)
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or https://www.apache.org/licenses/LICENSE-2.0)

at your option.
