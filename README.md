# Forge

**A cross-platform 2D CAD engine with ECS-backed geometry, GPU picking, snapping, undo/redo, and a 60fps wgpu render pipeline.**

[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](#license)
[![Rust](https://img.shields.io/badge/rust-1.87+-orange)](https://www.rust-lang.org)
[![Status](https://img.shields.io/badge/status-v0.2.0-yellow)](https://github.com/levu/forge)

Forge is a from-scratch CAD engine written in Rust. It combines an ECS architecture for geometric data, a wgpu-based GPU renderer, a nom command parser, and an egui immediate-mode UI to deliver a responsive drafting experience with selection, snapping, modify commands, and undo/redo.

---

## Features

- **Native cross-platform window** — runs on Linux (X11/Wayland), macOS, and Windows via winit 0.30.
- **wgpu 2D render pipeline** — orthographic camera, adaptive grid, per-entity geometry rendering, GPU picking, and selection highlights.
- **ECS data model** — entities and components stored in hecs, queried per frame for batching.
- **Command-driven interaction** — type commands at the command line or click toolbar buttons.
- **Interactive viewport** — middle-drag to pan, scroll to zoom (zoom toward mouse pointer).
- **Selection system** — single click (GPU picking), window selection (crossing/enclosing), select-all, deselect.
- **GPU picking** — entity ID framebuffer readback for pixel-perfect selection under the cursor.
- **Snapping engine** — 7 snap types with magnetic cursor and visual markers:
  - Endpoint, Midpoint, Center, Nearest, Perpendicular, Tangent, Grid
- **Spatial indexing** — R-tree (rstar) for snap queries, window selection, and view culling.
- **Modify commands** — ERASE, MOVE, COPY, ROTATE, SCALE, MIRROR, OFFSET.
- **Undo/Redo** — delta-based command journal with configurable depth (default 1000).
- **Grid overlay** — major/minor grid lines with axis markers, auto-regenerated on camera change.
- **UI chrome** — status bar (coordinates, zoom, snap indicators, selection count), command line, toolbar, property inspector.

### Primitives

| Primitive | Status |
|-----------|--------|
| `LINE` | Implemented and tested |
| `CIRCLE` | Implemented and tested |
| `ARC` | Implemented and tested |
| `PLINE` | Implemented and tested |

---

## Prerequisites

- **Rust 1.87+** — the minimum supported Rust version (MSRV). Install via [rustup](https://rustup.rs/).
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

The application opens a 1280x720 window titled "Forge v0.2.0".

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
| Select entity | Left-click (GPU picking) |
| Window select | Left-click + drag (left→right = enclosing blue, right→left = crossing green) |
| Deselect all | Click empty space |
| Cancel command | Escape |
| Undo | Ctrl+Z |
| Redo | Ctrl+Y |

### Snap behavior

When snapping is enabled (default), the cursor magnetises to geometric features as you move near them. Yellow markers indicate the active snap:

| Snap type | Marker shape | Priority |
|-----------|-------------|----------|
| Endpoint | Square | Highest (0) |
| Midpoint | Triangle | (1) |
| Center | Circle | (2) |
| Grid | Crosshair | (3) |
| Perpendicular | Perpendicular symbol | (4) |
| Tangent | Tangent symbol | (5) |
| Nearest | Hourglass | Lowest (6) |

Snap indicators appear in the status bar showing which types are active. The status bar also displays the current cursor coordinates, zoom level, and selection count.

### Command line

The command line panel sits at the bottom of the window, above the status bar. Type commands and press Enter to execute them.

**Draw commands:**

```
LINE 0,0 100,100              Draw a single segment from (0,0) to (100,100)
LINE 0,0 100,0 100,100        Accumulate points, Enter to commit (3 segments)
LINE                          Start interactive mode, pick points with clicks
L 0,0 100,100                 Short alias for LINE
U                             Undo the last point during interactive LINE
CIRCLE 50,50 25               Circle with centre (50,50) and radius 25
ARC 0,0 50 0 90               Arc at centre (0,0), radius 50, 0 to 90 degrees
PLINE 0,0 100,0 100,100 C     Closed polyline (rectangle) with close token
```

**Modify commands (operate on current selection):**

```
ERASE                         Delete all selected entities
MOVE                          Displace selected entities (base point → second point)
COPY                          Duplicate selected entities at an offset
ROTATE                        Rotate selected entities around a base point
SCALE                         Scale selected entities relative to a base point
MIRROR                        Reflect selected entities across a mirror line
OFFSET                        Create parallel copies at a specified distance
```

Modify commands can also be launched from the toolbar buttons.

### Selection

Click an entity to select it (single-click selection uses GPU picking — pixel-perfect hit detection). Selected entities are highlighted with a blue tint and thicker lines. Click empty space to deselect all.

Drag left to right to perform an **enclosing** selection (blue rectangle) — only entities fully inside the rectangle are selected. Drag right to left for **crossing** selection (green rectangle) — any entity intersecting the rectangle is selected.

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
┌─────────────────────────────────────────────────────────────┐
│                    APPLICATION LAYER (v0.2.0)               │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
│  │   Command   │  │   History   │  │   Selection         │  │
│  │   System    │  │   (Undo/    │  │   Manager           │  │
│  │             │  │   Redo)     │  │   (Selected set)    │  │
│  └──────┬──────┘  └──────┬──────┘  └──────────┬──────────┘  │
│         │                │                    │              │
│  ┌──────┴────────────────┴────────────────────┴──────────┐  │
│  │              SNAP ENGINE                                │  │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐   │  │
│  │  │  Candidate  │  │  Tolerance  │  │   Visual    │   │  │
│  │  │  Generator  │  │  Filter     │  │   Feedback  │   │  │
│  │  └─────────────┘  └─────────────┘  └─────────────┘   │  │
│  └─────────────────────────────────────────────────────────┘  │
│                          │                                  │
├──────────────────────────┼──────────────────────────────────┤
│                          │                                  │
│  ┌───────────────────────▼──────────────────────────────┐  │
│  │              RENDER LAYER (wgpu 29)                   │  │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐   │  │
│  │  │   Entity    │  │   Picking   │  │  Selection  │   │  │
│  │  │  Renderer   │  │  Pass (ID)  │  │  Highlight  │   │  │
│  │  └─────────────┘  └─────────────┘  └─────────────┘   │  │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐   │  │
│  │  │  Window     │  │   Grid      │  │  Snap       │   │  │
│  │  │  Select     │  │  Renderer   │  │  Markers    │   │  │
│  │  └─────────────┘  └─────────────┘  └─────────────┘   │  │
│  └────────────────────────────────────────────────────────┘  │
│                          │                                  │
├──────────────────────────┼──────────────────────────────────┤
│  ┌───────────────────────▼──────────────────────────────┐  │
│  │           SPATIAL INDEX (rstar R-tree)                  │  │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐   │  │
│  │  │   Entity    │  │   Window    │  │   Nearest   │   │  │
│  │  │   Insert    │  │   Query     │  │   Neighbor  │   │  │
│  │  └─────────────┘  └─────────────┘  └─────────────┘   │  │
│  └────────────────────────────────────────────────────────┘  │
│                          │                                  │
├──────────────────────────┼──────────────────────────────────┤
│                          │                                  │
│  ┌───────────────────────▼──────────────────────────────┐  │
│  │              PLATFORM LAYER (winit)                  │  │
│  │  ┌─────────────┐  ┌─────────────┐  ┌────────────┐  │  │
│  │  │   Window    │  │   Events    │  │   Input    │  │  │
│  │  │  Creation   │  │   Loop      │  │   Mapping  │  │  │
│  │  └─────────────┘  └─────────────┘  └────────────┘  │  │
│  └───────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

### Layers

1. **Application Layer** — `ForgeApp` owns the ECS world (`hecs`), command state, undo/redo history, selection manager, snap engine, spatial index, UI system (`egui`), and input mapper. It orchestrates the per-frame render pipeline, picking lifecycle, and command dispatch.

2. **Render Layer** — wgpu-based 2D pipeline with passes for grid, entities (with selection tint), picking (offscreen ID framebuffer), window selection rectangle, snap markers, and UI overlay. The camera uniform is shared across all shaders at bind group 0.

3. **Spatial Index** — rstar R-tree wrapping entity bounding boxes. Used by the snap engine for nearest-neighbour queries and by window selection for range queries (enclosed/intersecting). Lazy rebuild after entity modifications.

4. **Platform Layer** — winit 0.30 provides window creation, event loop (`ApplicationHandler` trait), and raw input events. The input mapper translates these into application-level actions with snap integration.

### Key crates

| Crate | Role |
|-------|------|
| `winit` 0.30 | Windowing and event loop |
| `wgpu` 29 | GPU abstraction (Vulkan/Metal/DX12) |
| `hecs` 0.10 | ECS framework for geometry storage |
| `nalgebra` 0.34 | Linear algebra (view-projection matrices) |
| `nom` 8 | Command-line parser combinators |
| `rstar` 0.12.2 | R-tree spatial index for snap / window queries |
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
├── app.rs               ForgeApp: owns World, RenderState, CommandState,
│                        SelectionManager, SnapEngine, SpatialIndex, History
├── ecs/                 ECS components and resources
│   ├── components.rs    LineData, CircleData, ArcData, PolylineData,
│   │                    Renderable, Position, Selected, SnapTarget
│   └── resources.rs     CameraState, GridConfig, InputState,
│                        SnapConfig, SelectionConfig
├── geometry/            Math and geometric types
│   ├── point.rs         Point2D (f64 precision)
│   ├── bounds.rs        BoundingBox2D with union operations
│   ├── vector.rs        Vector2D
│   └── transform.rs     Transform2D
├── commands/            Command system
│   ├── mod.rs           Command trait, CommandState, PendingModifyCommand
│   ├── parser.rs        nom v8 grammar (LINE, CIRCLE, ARC, PLINE)
│   ├── line_cmd.rs      LINE implementation (buffer-then-commit)
│   ├── circle_cmd.rs    CIRCLE implementation
│   ├── arc_cmd.rs       ARC implementation
│   ├── polyline_cmd.rs  Polyline implementation
│   ├── erase_cmd.rs     ERASE — removes selected entities
│   ├── move_cmd.rs      MOVE — displaces selected entities
│   ├── copy_cmd.rs      COPY — duplicates selected entities
│   ├── rotate_cmd.rs    ROTATE — rotates selected entities
│   ├── scale_cmd.rs     SCALE — scales selected entities
│   ├── mirror_cmd.rs    MIRROR — reflects selected entities
│   └── offset_cmd.rs    OFFSET — parallel copy at distance
├── selection/           Selection system (NEW in v0.2.0)
│   ├── mod.rs           SelectionManager, SelectionMode
│   ├── picking.rs       PickingPass (GPU entity ID readback)
│   └── window_select.rs WindowSelectState (crossing/enclosing)
├── snap/                Snap engine (NEW in v0.2.0)
│   ├── mod.rs           SnapEngine, SnapType, SnapResult
│   ├── candidate.rs     SnapCandidate trait, per-entity-type impls
│   ├── filter.rs        Tolerance filtering, priority ranking
│   └── visual.rs        SnapMarkerRenderer (screen-space billboards)
├── history/             Undo/redo (NEW in v0.2.0)
│   ├── mod.rs           History stack, EntityMapping, apply_entity_remapping
│   ├── transaction.rs   Transaction (label + Vec<AtomicOp>)
│   └── ops.rs           AtomicOp enum (Spawn, Despawn, SetComponent)
├── spatial/             Spatial index (NEW in v0.2.0)
│   └── mod.rs           SpatialIndex (rstar R-tree wrapper)
├── render/              wgpu render pipeline
│   ├── mod.rs           RenderState, pipeline orchestration
│   ├── camera.rs        OrthographicCamera, compute_view_proj_matrix
│   ├── grid.rs          GridRenderer (vertex-buffered line list)
│   ├── entity_renderer.rs  Per-type batching, selection tint (is_selected attr)
│   ├── picking_renderer.rs Offscreen ID framebuffer (NEW)
│   ├── selection_renderer.rs Selection highlight passes (NEW)
│   ├── window_select_renderer.rs Translucent rectangle renderer (NEW)
│   ├── pipeline.rs      Reserved
│   └── shaders/         WGSL shaders
│       ├── shared.wgsl      Camera uniform (view-proj mat4x4)
│       ├── grid.wgsl        Grid lines (major/minor/axes)
│       ├── entity.wgsl      Geometry with selection tint
│       ├── picking.wgsl     Entity ID → u32 (flat output)
│       ├── highlight.wgsl   Selection overlay (placeholder)
│       ├── window_select.wgsl  Translucent rect (green/blue)
│       └── snap_marker.wgsl Yellow screen-space markers
├── ui/                  egui panels
│   ├── mod.rs           UiSystem, UiOutput
│   ├── status_bar.rs    Coordinates, zoom, snap indicators, selection count
│   ├── command_line.rs  Text input with history and error display
│   ├── toolbar.rs       Draw + modify command buttons
│   └── property_panel.rs  Property inspector (stub)
├── input/               Input abstraction
│   ├── mod.rs           InputMapper: winit events → InputActions (snap-aware)
│   ├── camera_control.rs  Pan and zoom camera manipulation
│   └── command_input.rs Reserved
└── util/                Utilities
    ├── color.rs         RGBA Color (f32, bytemuck Pod/Zeroable)
    └── error.rs         ForgeError enum
```

---

## Rendering pipeline

Each frame executes these passes in order:

1. **Picking resolve** — read back the previous frame's GPU picking result (if any). Maps staging buffer, decodes entity ID, updates selection.
2. **Clear** — fill the framebuffer with `CameraState.clear_color` (dark grey `#2B2B2B`).
3. **Grid** — draw major/minor grid lines and X/Y axes as a single `LineList` draw call. Regenerated when the camera zoom or position changes by 10% or more.
4. **Window select rectangle** — if the user is dragging a selection rectangle, draw a translucent filled quad with border (blue for enclosing, green for crossing).
5. **Entities** — all ECS entities with a `Renderable` component, batched by type. Selected entities render with a blue tint (`mix(color, selection_blue, 0.3)`) and thicker lines via a per-instance `is_selected: u32` vertex attribute.
6. **Picking pass** (conditional) — if a pick was requested (mouse click), render all entities to an offscreen `Rgba32Uint` texture. Each pixel encodes the entity's instance index as a flat u32. Result is copied to a staging buffer for next-frame readback.
7. **Snap markers** — if a snap result is active, render yellow screen-space billboard quads (square, triangle, circle, or crosshair depending on snap type).
8. **UI overlay** — egui panels (status bar, command line, toolbar, property panel).

All shaders share a single camera uniform at `@group(0) @binding(0)`.

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

The `LINE` command uses a **buffer-then-commit** pattern: points accumulate in memory and are spawned into the ECS only on confirmation. Cancelling simply drops the buffer — no entities to clean up.

Modify commands (ERASE, MOVE, COPY, ROTATE, SCALE, MIRROR, OFFSET) capture the current selection at command start, build a `Transaction` containing `AtomicOp` variants, and push it onto the `History` stack upon completion. This enables full undo/redo for every modify operation.

The nom-based parser supports all four draw command types (`LINE`, `CIRCLE`, `ARC`, `PLINE`) with single-letter aliases (`L`, `C`, `A`, `PL`) and partial argument specification. Commands can be invoked with inline arguments or in interactive mode.

### Undo/Redo

Every command that modifies the ECS world produces a `Transaction` — a list of `AtomicOp` variants (Spawn, Despawn, SetComponent). The undo stack stores up to 1000 transactions by default. Undo replays operations in reverse; redo applies them forward. Entity handle remapping via `EntityMapping` ensures selection and spatial index stay consistent after re-spawns during undo/redo.

---

## Performance targets (v0.2.0)

| Metric | Target |
|--------|--------|
| Viewport FPS | >= 60 fps (<= 16.6 ms frame time) |
| Entity count | 10,000 lines at 60 fps (stress test) |
| Snap query | < 2 ms (nearest-neighbour + 6 candidate evaluations) |
| Picking readback | < 5 ms (GPU→CPU pixel copy + decode) |
| Window select query | < 5 ms (R-tree range query for 1000 entities) |
| Undo/redo | < 10 ms (transaction replay for 100 ops) |
| Spatial index rebuild | < 50 ms (bulk-load 10,000 entities) |
| Modify command (MOVE 1000) | < 16 ms (position updates + spatial index dirty) |
| Startup time | < 2 seconds |
| Command dispatch | < 1 ms (Enter → entity spawn) |
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
pub mod selection;
pub mod snap;
pub mod history;
pub mod spatial;
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
| **v0.1.0** | Viewport foundation: window, render pipeline, ECS, LINE command, grid, UI chrome |
| **v0.2.0** (current) | Selection system, GPU picking, snapping engine, spatial index, modify commands (ERASE, MOVE, COPY, ROTATE, SCALE, MIRROR, OFFSET), undo/redo |
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
