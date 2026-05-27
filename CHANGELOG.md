# Changelog

All notable changes to the Forge project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] — 2026-05-27

### Added

#### Selection System
- `SelectionManager` with `HashSet<hecs::Entity>` + `Selected` marker component sync.
- Single-click selection with GPU picking (entity ID framebuffer readback via
  `Rgba32Uint` offscreen texture, two-phase async protocol).
- Window selection: left-drag with enclosing (blue, left→right) and crossing
  (green, right→left) modes, querying the spatial index.
- Selection visual feedback: per-instance `is_selected` attribute tinting entities
  blue, plus translucent selection rectangle renderer.

#### Snap Engine
- 7 snap types: Endpoint, Midpoint, Center, Nearest, Perpendicular, Tangent, Grid.
- `SnapCandidate` trait with per-entity-type candidate generation (Line, Circle,
  Arc, Polyline).
- Tolerance filtering and priority ranking (Endpoint highest, Nearest lowest).
- Screen-space billboard markers (yellow): square (Endpoint), triangle (Midpoint),
  circle (Center), crosshair (Nearest/Perpendicular/Tangent/Grid).
- Snap integration with `InputMapper` — world coordinates are snapped before
  any action is emitted.

#### Spatial Index (rstar 0.12)
- `SpatialIndex` wrapping `rstar::RTree<SpatialEntry>` with nearest-neighbor,
  enclosed-in, and intersecting queries.
- Lazy rebuild via `dirty` flag for batch entity modifications.
- `BoundingBox2D`-based envelopes for all geometry types.

#### History (Undo/Redo)
- `History` with undo/redo stacks (`VecDeque<Transaction>`), configurable depth
  (default 1000), and standard branching semantics.
- `Transaction` with human-readable label + list of `AtomicOp`s.
- `AtomicOp` enum covering Spawn, Despawn, and Set variants for all entity types
  (Line, Circle, Arc, Polyline, Position).
- `EntityMapping` for stale handle remapping after entity re-spawn during undo/redo.
- `apply_entity_remapping()` to fix up `SelectionManager` and `SpatialIndex` after
  undo/redo cycles.
- Ctrl+Z (undo) and Ctrl+Y (redo) keyboard shortcuts, guarded against egui focus.

#### Modify Commands
- **ERASE** — removes selected entities with `Despawn*` transaction.
- **MOVE** — displaces selected entities by a vector, two-point interaction.
- **COPY** — spawns duplicates at offset, updates selection to new entities.
- **ROTATE** — rotates entities around a base point by a specified angle.
- **SCALE** — scales entities around a base point by a factor.
- **MIRROR** — reflects entities across a user-specified mirror line.
- **OFFSET** — creates parallel lines/concentric circles at a distance.
- All commands implement the `Command` trait, build `Transaction`s for history,
  and integrate with snap engine for point input.
- Toolbar buttons for all 7 modify commands.

#### Rendering Updates
- Per-instance `is_selected` vertex attribute for selection tint (avoids uniform
  buffer bloat per-entity).
- Dedicated picking WGSL shader (`picking.wgsl`) with flat `u32` entity ID output.
- `PickingPass` with offscreen `Rgba32Uint` texture, two-phase async readback,
  and per-entity vertex generation.
- `SnapMarkerRenderer` with 3 pipelines (TriangleStrip/TriangleList/LineList) and
  pre-computed billboard geometries.
- `WindowSelectRenderer` with translucent fill + border per mode.
- Updated entity WGSL shader with `mix()` selection tint.

#### Integration
- Full event-loop wiring: picking lifecycle, window select drag, snap integration
  in `InputMapper`, command dispatch, undo/redo shortcuts.
- Status bar: active snap type indicators (yellow labels) and selection count.
- Toolbar: modify command buttons for ERASE, MOVE, COPY, ROTATE, SCALE, MIRROR,
  OFFSET.
- `ForgeApp` owns all new subsystems (`SelectionManager`, `SnapEngine`,
  `SpatialIndex`, `History`) with initialization in `new()`.
- `rstar = "0.12.2"` dependency added.

### Deferred to v0.3.0+
- Ctrl+click multi-select modifier.
- Grip editing (stretch via handles).
- Layer system.
- Incremental spatial index updates (full rebuild is acceptable for v0.2.0).
- OFFSET with polyline corner trimming and self-intersection handling.

## [0.1.0] — 2026-05-23

### Added

#### Render Pipeline (wgpu 29)
- wgpu-based 2D render pipeline with orthographic camera, surface management, and
  frame submission via wgpu 29's `CurrentSurfaceTexture` API.
- Orthographic camera (`OrthographicCamera` / `CameraState`) with Y-up world
  coordinates, Y-down screen coordinates, and zoom-matrix encoding designed so
  the projection alone captures camera position (no separate view translation).
- Coordinate conversion helpers (`screen_to_world`, `world_to_screen`) with
  zero-viewport guards and zoom clamping.
- Shared camera uniform buffer (mat4x4<f32>) at bind group 0, consumed by all
  shaders (grid, entities, and egui overlay).
- Grid renderer (`GridRenderer`) with vertex-buffered major/minor grid lines,
  axis markers (X=0, Y=0), 10 % margin, lazy regeneration on ≥10 % camera
  shift, and 100K-vertex safety cap.
- Entity renderer (`EntityRenderer`) with four dedicated WGSL pipelines per
  primitive type (LineList for lines/polylines, LineStrip for circles/arcs),
  per-type staging buffers that double on overflow, and reusable scratch buffers
  to minimise per-frame allocations.
- Three WGSL shaders: `grid.wgsl`, `entity.wgsl` (shared by all entity types),
  and `shared.wgsl` (camera uniform struct).
- Surface loss recovery (reconfiguration + redraw on `Lost` / `Outdated`).
- Window resize handling (surface reconfiguration + grid invalidation).

#### Entity-Component System (hecs 0.10)
- ECS world (`hecs::World`) as the core geometry store.
- Component types: `LineData`, `CircleData`, `ArcData`, `PolylineData` (all
  with absolute world-space coordinates, f64 precision).
- `Renderable` marker component for entity render participation.
- `Position` component (placeholder for v0.2.0+ transforms / instancing).
- Singleton resources: `CameraState` (target, zoom, viewport, clear colour),
  `GridConfig` (spacing, colours, visibility), `InputState` (mouse, buttons,
  modifiers).

#### Geometry Types
- `Point2D` — f64-precision 2D point with `to_f32_array()` for GPU upload.
- `BoundingBox2D` — AABB with `union()` (empty-safe) and `center()`.
- `Vector2D` and `Transform2D` modules (reserved — v0.2.0+).

#### Command System
- `Command` trait with interactive multi-step state machine (prompt, input,
  preview, cancel).
- `CommandState` manager with active command, history, error tracking, pending
  dispatch queue, and deferred cancel (`cancel_requested` / `process_pending_cancel`).
- Command enums: `CommandInput` (Point, Text, Distance, Angle, Cancel, Confirm)
  and `CommandResult` (Continue, Complete, Error, Cancelled).
- `PreviewEntity` descriptor type for rubber-band preview (rendering deferred
  to v0.2.0+).

#### Command Parser (nom 8)
- Zero-copy nom v8 command grammar for LINE, CIRCLE, ARC, and PLINE.
- Case-insensitive command names with single-letter aliases (L, C, A, PL).
- `parse_point()` — parses `"x,y"` with optional whitespace and scientific
  notation via nom's `float` combinator.
- Coordinate types: positive, negative, scientific, large values, tabs between
  tokens.
- PLINE close token detection (`C`) with whitespace/EOF guard to prevent false
  matches on coordinate text (e.g. `C10,20`).
- Partial-argument parsing (`LINE 0,0` → start point only).
- 30+ unit tests covering full command grammar, edge cases, and error paths.

#### LINE Command
- Fully interactive LINE command with buffer-then-commit design: points
  accumulate in a pending buffer and only spawn ECS entities on `Confirm` /
  `Complete` — nothing is spawned on `Cancel`.
- Undo support (`U` text input) — pops the last accumulated point.
- Auto-commit when `steps_remaining() == 0` (e.g. `LINE 0,0 100,100`).
- Multi-segment LINE support (3+ points on Confirm → N-1 segments spawned).
- 20+ unit tests covering state machine, multi-segment, undo, empty-buffer
  safety, and error paths.

#### CIRCLE, ARC, PLINE Commands (Parsers Only)
- Command parsers fully implemented for all three commands with comprehensive
  argument extraction.
- Command stubs return user-visible error messages (`"not yet implemented (v0.2.0+)"`).
- Full command dispatch routing in `ForgeApp::dispatch_command_text`.

#### Input System (winit 0.30)
- `InputMapper` translating winit `WindowEvent`s into `InputAction`s.
- Camera pan via middle-button drag with first-event baseline to prevent
  position jumps.
- Camera zoom via scroll wheel with pixel-to-line-delta normalisation,
  delta clamping (±10), and zoom-toward-mouse-pointer pivot adjustment.
- Click dispatch to active command for point picking.
- Keyboard modifier tracking (Shift, Ctrl, Alt) and Escape/Enter routing.
- `CameraControl` module with defensive zoom clamping (`[0.0001, 100_000.0]`)
  and safe zoom-at-zero handling.
- 8 unit tests covering pan/zoom math, clamping, and mapper construction.

#### UI Chrome (egui 0.34 / egui-wgpu / egui-winit)
- `UiSystem` owning egui context and winit integration state.
- Four-panel layout: status bar (bottom), command line (above status bar),
  toolbar (left), property inspector (right).
- Status bar with live mouse coordinates (4 decimal places), zoom percentage,
  and version string. Pure helper functions (`format_coord`, `format_zoom`)
  with 10 unit tests.
- Command-line text input (`text_edit_singleline`) with Enter submission,
  Escape cancellation, command history (capped at 1000 entries), and
  error display in red. 12 unit tests covering prompt resolution, submit
  logic, cancel logic, and history management.
- Toolbar panel (stub — unicode icon placeholders for LINE, CIRCLE, ARC).
- Property inspector panel (stub — "v0.2.0+" placeholder).
- HiDPI / `ScaleFactorChanged` handling.

#### Application Loop (winit 0.30 ApplicationHandler)
- `ForgeAppHandler` with `ApplicationHandler` trait implementation.
- `resumed()` — window creation with macOS resume safety (drops prior GPU
  resources when re-activated).
- `window_event()` — lifecycle events (CloseRequested, Destroyed, Resized,
  ScaleFactorChanged, RedrawRequested), egui-first event consumption,
  pending command dispatch, deferred cancel processing, input mapping
  and action dispatch.
- `about_to_wait()` — continuous rendering via `request_redraw()` every
  idle frame for 60 fps viewport refresh.
- Synchronous GPU initialisation via `pollster::block_on` with startup
  timing note.

#### Utility Types
- `Color` — RGBA f32 with `from_hex()` const constructor, predefined
  constants (WHITE, BLACK, GRAY_DARK, GRAY_MEDIUM, GRAY_LIGHT),
  bytemuck Pod/Zeroable derives for GPU upload.
- `ForgeError` — typed error enum covering GPU, Surface, Command, Parse,
  and IO errors, with From impls for wgpu errors.

#### Build & Configuration
- Cargo.toml entry with dependencies: winit 0.30, wgpu 29, hecs 0.10,
  nalgebra 0.34, nom 8, egui 0.34, egui-wgpu 0.34, egui-winit 0.34,
  thiserror 2, tracing 0.1, pollster 0.4, bytemuck 1.25.
- MSRV: Rust 1.87.
- Release profile: `opt-level = 3`, `lto = "thin"`, `codegen-units = 1`,
  `strip = true`.

### Deferred to v0.2.0+

The following features are **not included** in this release and are planned
for v0.2.0+ (see the project specification for details):

- Selection system
- Snapping engine
- Modify commands (MOVE, ERASE, etc.)
- Undo/redo
- Full CIRCLE command implementation (parser exists, interactive command is a stub)
- Full ARC command implementation (parser exists, interactive command is a stub)
- Full PLINE command implementation (parser exists, interactive command is a stub)
- Rubber-band preview geometry during active commands
- Pipeline cache
- Vector2D and Transform2D types
- Layer system (v0.3.0)
- File I/O (v0.5.0)
- WebAssembly build (v1.2.0)
- Constraint solving (v0.7.0)

### Known Limitations

- CIRCLE, ARC, and PLINE commands parse successfully but return
  `"not yet implemented (v0.2.0+)"` when dispatched.
- Toolbar and property panel are visual stubs only (no interactive buttons,
  no entity inspection).
- Rubber-band line/circle/arc preview during command execution is not
  rendered (deferred to v0.2.0+).
- Occlusion-aware rendering not implemented — GPU cycles may be wasted
  when the window is minimised or hidden.
- PipelineCache is a reserved module with no implementation.
- Benchmark targets (10,000 entities at 60 fps) are tracked via manual
  profiling; headless benchmarking is not yet wired.

### Performance Targets

| Metric | Target | Status |
|--------|--------|--------|
| Viewport FPS | ≥60 fps | Continuous rendering loop active |
| Startup Time | <2 s | Synchronous GPU init (see v0.2.0+ for async) |
| Entity Count | 10,000 at 60 fps | Buffer-doubling strategy in place; profiling pending |
| Camera Pan/Zoom | <1 ms | Frame-delta transform, no drops during interaction |

## [0.0.0] — Project inception

- Repository initialised.
- Project specification (`.docs/.specs/v0.1.0.md`) created with architecture
  overview, crate selection, module structure, and data-structure design.
- Dependencies resolved: winit 0.30, wgpu 29, hecs 0.10, nalgebra 0.34,
  nom 8, egui 0.34, egui-wgpu 0.34, egui-winit 0.34, thiserror 2,
  tracing 0.1, pollster 0.4, bytemuck 1.25.

[0.1.0]: https://github.com/forge-rs/forge/releases/tag/v0.1.0
[0.0.0]: https://github.com/forge-rs/forge
