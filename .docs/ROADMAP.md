# Forge Roadmap

> **Versioned development plan for Forge — a Rust-native, cross-platform CAD engine.**

---

## Versioning Philosophy

Forge follows [Semantic Versioning](https://semver.org/):
- **MAJOR** — Breaking architectural changes or milestone releases
- **MINOR** — Feature additions, significant capability increments
- **PATCH** — Bug fixes, performance improvements, minor enhancements

Each minor version represents a **milestone** with defined goals, deliverables, and acceptance criteria.

---

## Legend

| Status | Icon |
|--------|------|
| Planned | 🔵 |
| In Progress | 🟡 |
| Completed | 🟢 |
| Blocked | 🔴 |
| Deferred | ⚪ |

---

## v0.1.0 — Foundation *(The Viewport)* `🟢 Completed`

**Goal:** A window that can display geometry. Pan, zoom, and draw basic entities.

### Core Engine
- [x] 🟢 Windowing layer (`winit` + `wgpu`) — cross-platform window creation
- [x] 🟢 Basic render pipeline — clear color, 2D orthographic camera
- [x] 🟢 ECS architecture setup (`hecs`) — entity storage and component queries
- [x] 🟢 Coordinate system — world space, screen space, grid snapping (visual only)
- [x] 🟢 Input handling — mouse move, click, scroll, keyboard shortcuts

### Geometry Primitives
- [x] 🟢 `Line` entity — two-point definition
- [x] 🟢 `Circle` entity — center + radius
- [x] 🟢 `Arc` entity — center, radius, start angle, end angle
- [x] 🟢 `Polyline` entity — ordered sequence of vertices

### UI Chrome
- [x] 🟢 Command line parser (`nom`) — tokenize and dispatch commands
- [x] 🟢 Status bar — current coordinates, active command, snap mode
- [x] 🟢 Toolbar placeholder — icon buttons for draw commands
- [x] 🟢 Property inspector stub — read-only entity properties

### Deliverables
- [x] 🟢 Native binary runs on Linux, macOS, Windows
- [x] 🟢 60fps viewport on a mid-range GPU
- [x] 🟢 Command: `LINE`, `CIRCLE`, `ARC`, `PLINE`

### Target Date
- **Month 1–2**

---

## v0.2.0 — Editing & Precision *(The Draftsman)* `🟢 Completed`

**Goal:** Modify geometry with precision. Snapping, selection, and basic transforms.

### Selection System
- [x] 🟢 Click selection — single entity pick
- [x] 🟢 Window selection — crossing and enclosing rectangle
- [x] 🟢 Selection set — highlight, add/remove from set
- [x] 🟢 GPU picking — color-encoded framebuffer for hit testing

### Snapping Engine
- [x] 🟢 Endpoint snap
- [x] 🟢 Midpoint snap
- [x] 🟢 Center snap
- [x] 🟢 Nearest snap
- [x] 🟢 Perpendicular snap
- [x] 🟢 Tangent snap (circle/arc)
- [x] 🟢 Grid snap — configurable spacing
- [x] 🟢 Snap visualization — markers, tooltips, magnetic cursor

### Modify Commands
- [x] 🟢 `ERASE` — delete selected entities
- [x] 🟢 `MOVE` — translate by base point + displacement
- [x] 🟢 `COPY` — duplicate with offset
- [x] 🟢 `ROTATE` — pivot + angle
- [x] 🟢 `SCALE` — base point + scale factor
- [x] 🟢 `MIRROR` — mirror line
- [x] 🟢 `OFFSET` — parallel copy at distance

### Undo/Redo
- [x] 🟢 Command pattern for all mutations
- [x] 🟢 Undo stack — unlimited depth, memory-bounded
- [x] 🟢 Redo stack — invalidated on new command

### Deliverables
- [x] 🟢 GPU picking with async two-phase readback (request N, resolve N+1)
- [x] 🟢 R-tree spatial index (rstar) — snap queries, window selection, view culling
- [x] 🟢 Modify commands build Transaction → History stack for full undo/redo
- [x] 🟢 EntityMapping remaps stale handles after undo/redo (Selection + Spatial)

### Target Date
- **Month 3 — 2026-05-27 ✅**

---

## v0.3.0 — Organization *(The Layered Canvas)*

**Goal:** Organize drawings with layers, blocks, and property management.

### Layer System
- [ ] 🔵 Layer manager — create, rename, delete, reorder
- [ ] 🔵 Layer properties — color, linetype, linewidth, visibility, lock, freeze
- [ ] 🔵 Entity-to-layer assignment
- [ ] 🔵 Layer filtering and isolation
- [ ] 🔵 ByLayer / ByBlock property inheritance

### Block System
- [ ] 🔵 Block definition — grouped geometry with base point
- [ ] 🔵 Block insert — placement with position, scale, rotation
- [ ] 🔵 Nested blocks — block references inside block definitions
- [ ] 🔵 Block editor — in-place or dedicated mode
- [ ] 🔵 Explode block to constituent entities

### Property System
- [ ] 🔵 Property palette (`egui`) — live editing of entity attributes
- [ ] 🔵 Match properties — copy attributes between entities
- [ ] 🔵 Quick properties — hover tooltip with key data
- [ ] 🔵 Global property defaults

### Advanced Draw Commands
- [ ] 🔵 `RECTANGLE` — two-corner definition
- [ ] 🔵 `POLYGON` — center + radius + sides
- [ ] 🔵 `ELLIPSE` — major/minor axis
- [ ] 🔵 `SPLINE` — control point or fit point spline (Cubic Bezier)

### Deliverables
- [ ] Create a block library (door, window, furniture symbols)
- [ ] Layer-based visibility control in a 50-layer drawing

### Target Date
- **Month 4**

---

## v0.4.0 — Annotation *(The Document)*

**Goal:** Document drawings with dimensions, text, and hatch patterns.

### Dimensioning
- [ ] 🔵 Linear dimension — aligned, horizontal, vertical, rotated
- [ ] 🔵 Radial dimension — radius and diameter
- [ ] 🔵 Angular dimension — between two lines or three points
- [ ] 🔵 Ordinate dimension — X/Y datum
- [ ] 🔵 Dimension styles — text height, arrowheads, tolerance, precision
- [ ] 🔵 Associative dimensions — update when geometry changes

### Text & Annotations
- [ ] 🔵 Single-line text (`TEXT`)
- [ ] 🔵 Multi-line text (`MTEXT`) — paragraph formatting
- [ ] 🔵 Text styles — font, height, width factor, oblique angle
- [ ] 🔵 Leaders — arrow + text callout
- [ ] 🔵 Font rendering — `cosmic-text` + `glyphon` for wgpu

### Hatch & Fill
- [ ] 🔵 Pattern hatch — ANSI/ISO predefined patterns
- [ ] 🔵 Solid fill — uniform color
- [ ] 🔵 Gradient fill — linear, radial
- [ ] 🔵 Boundary detection — pick point or closed polyline
- [ ] 🔵 Hatch scale and angle control
- [ ] 🔵 Compute shader hatch rendering for performance

### Deliverables
- [ ] Fully annotated architectural plan with dimensions and hatches
- [ ] Text rendering at 4K with subpixel precision

### Target Date
- **Month 5**

---

## v0.5.0 — Interoperability *(The Bridge)*

**Goal:** Import and export industry-standard file formats.

### DXF I/O
- [ ] 🔵 DXF ASCII reader — full entity support for R14–2018
- [ ] 🔵 DXF binary reader
- [ ] 🔵 DXF writer — round-trip fidelity
- [ ] 🔵 Layer, block, dimension style translation
- [ ] 🔵 Error recovery — skip malformed sections, report warnings

### Native Format
- [ ] 🔵 `.forge` file specification v1
- [ ] 🔵 `serde` + `rkyv` serialization — fast save/load
- [ ] 🔵 `zstd` compression — compact files
- [ ] 🔵 Schema versioning — backward-compatible loading
- [ ] 🔵 Incremental save — only dirty entities

### Export
- [ ] 🔵 SVG export — vector for web/print
- [ ] 🔵 PDF export — single page and multi-sheet
- [ ] 🔵 PNG/JPEG export — raster image with resolution control

### Deliverables
- [ ] Open and edit a real-world DXF floor plan (≥1000 entities)
- [ ] Save/load round-trip with <1% data loss

### Target Date
- **Month 6**

---

## v0.6.0 — Advanced Geometry *(The Craftsman)*

**Goal:** Complex editing operations and geometric precision.

### Advanced Modify
- [ ] 🔵 `TRIM` — trim to boundary edges
- [ ] 🔵 `EXTEND` — extend to boundary edges
- [ ] 🔵 `FILLET` — round corner with radius
- [ ] 🔵 `CHAMFER` — bevel corner with distances
- [ ] 🔵 `JOIN` — combine collinear lines, arcs, polylines
- [ ] 🔵 `BREAK` — split at point or gap
- [ ] 🔵 `STRETCH` — move vertices within crossing window

### Geometric Operations
- [ ] 🔵 Boolean 2D — union, intersect, subtract (`clipper-rs`)
- [ ] 🔵 Area and perimeter calculation
- [ ] 🔵 Centroid and moment of inertia
- [ ] 🔵 Bounding box (world and oriented)
- [ ] 🔵 Collision detection — point-in-polygon, overlap test

### Coordinate Systems
- [ ] 🔵 User Coordinate System (UCS) — custom origin and axes
- [ ] 🔵 Named views — save and restore camera positions
- [ ] 🔵 Viewports — multiple panes in desktop (future)

### Deliverables
- [ ] Boolean operations on complex polygonal shapes (≥100 vertices)
- [ ] Trim/extend with 100+ boundary edges in <100ms

### Target Date
- **Month 7**

---

## v0.7.0 — Parametric Design *(The Solver)*

**Goal:** Constraint-driven geometry with parametric relationships.

### Constraint Engine
- [ ] 🔵 Geometric constraints — coincident, parallel, perpendicular, horizontal, vertical
- [ ] 🔵 Dimensional constraints — distance, angle, radius, diameter
- [ ] 🔵 Constraint solver — graph-based dependency resolution (Gauss-Newton or Dogleg)
- [ ] 🔵 Constraint visualization — icons, preview, degrees of freedom
- [ ] 🔵 Auto-constrain — infer constraints from sketch

### Parameters & Expressions
- [ ] 🔵 Global parameters — named variables (e.g., `WallThickness = 200`)
- [ ] 🔵 Expression evaluator — arithmetic, functions, parameter references
- [ ] 🔵 Parameter table — spreadsheet-like editor
- [ ] 🔵 Driven dimensions — parametrically controlled

### Design Tables
- [ ] 🔵 Family of parts — variant configurations
- [ ] 🔵 Suppression states — enable/disable features per variant

### Deliverables
- [ ] Parametric door block — width, height, frame thickness as variables
- [ ] Solve 50-constraint sketch in <500ms

### Target Date
- **Month 8**

---

## v0.8.0 — 3D Viewing *(The Navigator)*

**Goal:** Display and navigate 3D geometry. Not yet full modeling.

### 3D Rendering
- [ ] 🔵 Perspective and orthographic cameras
- [ ] 🔵 Orbit, pan, zoom in 3D space
- [ ] 🔵 Standard views — top, front, right, isometric
- [ ] 🔵 View cube / navigation gizmo
- [ ] 🔵 Back-face culling and wireframe mode

### 3D Entity Display
- [ ] 🔵 3D faces and meshes
- [ ] 🔵 Extruded 2D profiles (visual only)
- [ ] 🔵 Point clouds — LOD rendering
- [ ] 🔵 Section planes — cut through 3D geometry

### Lighting & Shading
- [ ] 🔵 Basic Phong shading
- [ ] 🔵 Ambient + directional lights
- [ ] 🔵 Edge highlighting and silhouette
- [ ] 🔵 Material colors per entity

### Deliverables
- [ ] Navigate a 10,000-triangle mesh at 60fps
- [ ] Section plane with real-time update

### Target Date
- **Month 9**

---

## v0.9.0 — Extensibility *(The Platform)*

**Goal:** Allow users and third parties to extend Forge.

### Scripting
- [ ] 🔵 Embedded scripting language — `rhai` or Lua (`mlua`)
- [ ] 🔵 API surface — create, query, modify entities
- [ ] 🔵 Script editor — built-in with syntax highlighting
- [ ] 🔵 Macro recording — capture commands as script
- [ ] 🔵 Script palette — run, save, load user scripts

### WASM Plugin API
- [ ] 🔵 Plugin manifest — metadata, permissions, entry points
- [ ] 🔵 WASM sandbox — WASI runtime, memory isolation
- [ ] 🔵 Plugin commands — register custom commands
- [ ] 🔵 Plugin UI — embedded panels via web components
- [ ] 🔵 Plugin marketplace — discovery and installation (future)

### Automation
- [ ] 🔵 Batch processing — script + file list
- [ ] 🔵 Command-line interface (CLI) — headless operation
- [ ] 🔵 Export automation — scripted output pipelines

### Deliverables
- [ ] A community plugin that adds a new hatch pattern library
- [ ] CLI batch conversion: DXF → PDF for 100 files

### Target Date
- **Month 10**

---

## v1.0.0 — Stable Release *(The Launch)*

**Goal:** Production-ready 2D CAD for professionals.

### Polish
- [ ] 🔵 Performance audit — 10,000+ entity drawings at 60fps
- [ ] 🔵 Memory audit — no leaks, bounded undo, efficient ECS
- [ ] 🔵 Accessibility — keyboard-only workflow, screen reader support
- [ ] 🔵 Localization — i18n framework, initial EN/DE/FR/JA/ZH
- [ ] 🔵 Documentation — user manual, API docs, video tutorials

### Print & Layout
- [ ] 🔵 Paper space / model space
- [ ] 🔵 Layout tabs — multiple sheets per drawing
- [ ] 🔵 Viewports in layouts — scale and crop model views
- [ ] 🔵 Plot styles — CTB/STB, monochrome, grayscale
- [ ] 🔵 Printer configuration — paper size, orientation, margins

### Licensing & Distribution
- [ ] 🔵 Open-core license — core MIT/Apache, enterprise features commercial
- [ ] 🔵 Installer — Windows MSI, macOS DMG, Linux AppImage/Flatpak
- [ ] 🔵 Update mechanism — auto-check, delta updates
- [ ] 🔵 Telemetry (opt-in) — crash reports, performance metrics

### Deliverables
- [ ] Replace AutoCAD for 80% of 2D drafting use cases
- [ ] Community of 1,000+ active users

### Target Date
- **Month 12**

---

## v1.1.0 — Collaboration *(The Network)*

**Goal:** Real-time multi-user editing and cloud features.

### CRDT Sync
- [ ] 🔵 Operation-based CRDT document model
- [ ] 🔵 WebSocket sync server (Rust + `tokio-tungstenite`)
- [ ] 🔵 Presence — cursors, selections, user avatars
- [ ] 🔵 Conflict resolution — visual merge for overlapping edits
- [ ] 🔵 Offline mode — local queue, sync on reconnect

### Cloud Features
- [ ] 🔵 User accounts and authentication
- [ ] 🔵 Project hosting — cloud storage for `.forge` files
- [ ] 🔵 Version history — branching, tagging, rollback
- [ ] 🔵 Sharing — public links, permission levels

### Deliverables
- [ ] Two users edit the same drawing with <100ms latency
- [ ] Git-like history browser for design iterations

### Target Date
- **Month 14**

---

## v1.2.0 — Web Deployment *(The Browser)*

**Goal:** Run Forge in any modern web browser.

### WebAssembly Build
- [ ] 🔵 `wasm32-unknown-unknown` target compilation
- [ ] 🔵 `wasm-bindgen` FFI layer
- [ ] 🔵 WebGPU backend via `wgpu`
- [ ] 🔵 WebGL2 fallback for unsupported browsers

### Web UI
- [ ] 🔵 Browser-based shell — React/Vue/Svelte
- [ ] 🔵 File upload/download — `.forge`, DXF, PDF
- [ ] 🔵 Cloud sync integration
- [ ] 🔵 Mobile touch support — pinch zoom, two-finger pan

### Performance
- [ ] 🔵 Streaming geometry — chunked loading for large files
- [ ] 🔵 WASM SIMD — leverage `simd128` for math
- [ ] 🔵 Service worker — offline caching

### Deliverables
- [ ] Open and edit a 5,000-entity drawing in Chrome/Firefox/Safari
- [ ] Web version reaches 80% feature parity with desktop

### Target Date
- **Month 16**

---

## v1.3.0 — 3D Modeling *(The Modeler)*

**Goal:** Solid 3D CAD capabilities.

### 3D Kernel
- [ ] 🔵 BRep representation — faces, edges, vertices
- [ ] 🔵 NURBS surfaces — trimming, evaluation
- [ ] 🔵 Solid primitives — box, cylinder, sphere, cone, torus
- [ ] 🔵 Boolean 3D — union, subtract, intersect
- [ ] 🔵 Fillet and chamfer on edges
- [ ] 🔵 Shell and offset surfaces

### 3D Operations
- [ ] 🔵 Extrude — 2D profile to 3D solid
- [ ] 🔵 Revolve — profile around axis
- [ ] 🔵 Sweep — profile along path
- [ ] 🔵 Loft — between multiple profiles
- [ ] 🔵 Draft — tapered faces

### Mesh & Visualization
- [ ] 🔵 Mesh generation — triangulation for display
- [ ] 🔵 STL/OBJ export — 3D printing workflow
- [ ] 🔵 STEP/IGES import — industry interoperability

### Deliverables
- [ ] Model a mechanical part with 20+ features
- [ ] Export valid STL for 3D printing

### Target Date
- **Month 18**

---

## v1.4.0 — Enterprise *(The Professional)*

**Goal:** Features for teams, compliance, and large-scale workflows.

### Data Management
- [ ] 🔵 PDM/PLM integration — WebDAV, Git LFS, custom connectors
- [ ] 🔵 Drawing standards — company templates, layer standards
- [ ] 🔵 Batch plotting — multi-sheet output
- [ ] 🔵 Drawing comparison — visual diff between versions

### Compliance
- [ ] 🔵 Audit trail — who changed what, when
- [ ] 🔵 Digital signatures — drawing certification
- [ ] 🔵 Role-based access control
- [ ] 🔵 On-premise deployment option

### Advanced Rendering
- [ ] 🔵 Ray-traced rendering — photorealistic output
- [ ] 🔵 Material library — metals, plastics, glass
- [ ] 🔵 Environment maps — HDRI lighting
- [ ] 🔵 Animation — exploded views, assembly sequences

### Deliverables
- [ ] Enterprise pilot with 50+ seat deployment
- [ ] Photorealistic render of a complex assembly

### Target Date
- **Month 20**

---

## v2.0.0 — Ecosystem *(The Platform)*

**Goal:** Forge as a platform, not just a tool.

### Marketplace
- [ ] 🔵 Plugin store — discover, rate, install extensions
- [ ] 🔵 Block library marketplace — community symbols
- [ ] 🔵 Template marketplace — industry-specific starters
- [ ] 🔵 Monetization — revenue share with creators

### AI Assist
- [ ] 🔵 Smart dimensioning — auto-suggest dimensions
- [ ] 🔵 Drawing cleanup — detect and fix errors
- [ ] 🔵 Symbol recognition — raster to vector
- [ ] 🔵 Natural language commands — "draw a room 4x5 meters"

### Mobile Companion
- [ ] 🔵 iOS/Android viewer — lightweight, fast
- [ ] 🔵 Markup and redline — annotations on the go
- [ ] 🔵 AR overlay — view 3D models in physical space

### Deliverables
- [ ] 10,000+ active users across platforms
- [ ] Self-sustaining plugin ecosystem

### Target Date
- **Month 24**

---

## Appendix A: Tech Stack by Version

| Version | New Dependencies |
|---------|-----------------|
| v0.1.0 | `winit`, `wgpu`, `hecs`, `nalgebra`, `nom` |
| v0.2.0 | `rstar` (R-tree) |
| v0.3.0 | `egui`, `cosmic-text` |
| v0.4.0 | `resvg`, `printpdf` |
| v0.5.0 | `rkyv`, `zstd`, `serde` |
| v0.6.0 | `clipper-rs` |
| v0.7.0 | Custom solver or `nalgebra` optimization |
| v0.8.0 | — |
| v0.9.0 | `rhai` or `mlua`, `wasmtime` |
| v1.0.0 | `i18n-embed`, `tao` (Tauri) |
| v1.1.0 | `tokio-tungstenite`, `automerge-rs` |
| v1.2.0 | `wasm-bindgen`, `js-sys` |
| v1.3.0 | `truck` or `opencascade-rs` |
| v1.4.0 | Custom render/raytracer |

---

## Appendix B: Definition of Done

For each version to be considered complete:

1. **All features implemented** and unit-tested
2. **Integration tests** pass — real-world DXF round-trip
3. **Performance benchmarks** meet targets
4. **Documentation** updated — API docs, user guide, changelog
5. **Cross-platform CI** green — Linux, macOS, Windows
6. **No critical or high bugs** in issue tracker
7. **Release notes** published

---

*Forge Roadmap v1.0 — 2026-05-20*
