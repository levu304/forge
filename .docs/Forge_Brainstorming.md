# Forge: Cross-Platform CAD Renaissance

> **A Rust-native, cross-platform alternative to AutoCAD — inheriting its strengths, eliminating its weaknesses.**

---

## 1. Vision

Forge is a CAD engine core built in Rust that compiles to both native desktop (via Tauri/winit) and browser (via WebAssembly/WebGPU). The architecture separates a heavy Rust computational core from thin platform-specific UI shells, enabling true code sharing while exploiting Rust's zero-cost abstractions for geometry performance.

**Tagline:** *"The CAD engine that syncs, renders, and computes in Rust — everywhere."*

---

## 2. Inherited AutoCAD Strengths

| Feature | Implementation Strategy |
|---------|------------------------|
| **Precision Coordinate Systems** | Fixed-point decimal (`rust_decimal`) or rational number types for CAD-grade precision; avoid `f64` rounding in critical paths |
| **Command Line Interface** | Rust parser (`nom` or `winnow`) for command grammar; modal command system with autocomplete |
| **Layer System** | Entity-Component-System (ECS) architecture (`hecs`/`bevy_ecs`) where layers are tags/components |
| **Block/Insert System** | Instancing via ECS archetypes — one geometry definition, multiple transformed references |
| **Constraint Solver** | Rust-native geometric constraint engine (2D/3D) using graph-based dependency resolution |
| **Scripting/Extensibility** | Embedded DSL compiled to WASM sandbox; or Lua via `mlua` |
| **DWG Compatibility** | Partner with LibreCAD/OpenDesign Alliance (ODA) file specs; implement DWG/DXF readers in Rust |

---

## 3. AutoCAD Pain Points — Resolved

| AutoCAD Problem | Forge Solution |
|-----------------|---------------|
| **Subscription Tax** | Open-core model: free for individuals, paid enterprise features (cloud render, PLM integration) |
| **Windows Monopoly** | Rust cross-platform from day one: Linux, macOS, Windows, Web |
| **Bloat & Sluggishness** | Rust zero-cost + aggressive spatial indexing (BVH/R-tree via `rstar`); lazy loading of blocks |
| **File-based Collaboration** | Real-time CRDT-based document model; git-like branching for designs |
| **Steep Learning Curve** | Contextual UI that reveals complexity progressively; "Quick Action" palette for common tasks |
| **Resource Hog** | Memory-mapped file I/O (`memmap2`), incremental serialization, streaming geometry |
| **No Modern API** | gRPC/REST API + WASM plugin SDK; everything scriptable |

---

## 4. Architecture: Rust Core + Thin Shell

```
┌─────────────────────────────────────────────────────────────┐
│                    UI SHELL (Thin)                          │
│  ┌──────────────┐  ┌──────────────┐  ┌─────────────────┐  │
│  │   Desktop    │  │    Web       │  │   Mobile (future)│  │
│  │ (Tauri/Winit)│  │(WASM + WebGPU)│  │   (Tauri/iOS)   │  │
│  └──────┬───────┘  └──────┬───────┘  └────────┬────────┘  │
└─────────┼──────────────────┼────────────────────┼──────────┘
          │                  │                    │
          └──────────────────┼────────────────────┘
                             │
┌────────────────────────────▼──────────────────────────────┐
│                  CAD ENGINE CORE (Rust)                     │
│  ┌─────────────┐ ┌─────────────┐ ┌─────────────────────┐  │
│  │  Document   │ │  Geometry   │ │   Constraint Solver │  │
│  │   Model     │ │   Kernel    │ │       (Graph)       │  │
│  │  (CRDTs)    │ │ (NURBS/BRep)│ │                     │  │
│  └─────────────┘ └─────────────┘ └─────────────────────┘  │
│  ┌─────────────┐ ┌─────────────┐ ┌─────────────────────┐  │
│  │   Spatial   │ │  Rendering  │ │   I/O (DXF/DWG/   │  │
│  │   Index     │ │  (WGPU/Vulkan)│ │   STEP/IFC)       │  │
│  │ (R-Tree/BVH)│ │             │ │                     │  │
│  └─────────────┘ └─────────────┘ └─────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

---

## 5. Core Modules (All Rust)

| Module | Crate/Approach | Purpose |
|--------|---------------|---------|
| **Math/Linear Algebra** | `nalgebra`, `glam`, `ultraviolet` | Transform matrices, vectors, quaternions |
| **2D Geometry** | `geo`, `geos` (bindings), custom | Boolean ops, offset, convex hull |
| **3D Kernel** | Custom or `truck` (Rust BRep) | NURBS, solids, meshing |
| **Graphics** | `wgpu` | Cross-platform: Vulkan/Metal/DX12/WebGPU |
| **Windowing** | `winit` (native), `tao` (Tauri) | Event loop, input handling |
| **UI Framework** | `egui` (immediate mode) or `slint` | CAD-optimized property panels, toolbars |
| **Serialization** | `serde` + `rkyv`/`flatbuffers` | Fast save/load; `rkyv` for zero-copy |
| **Scripting** | `rhai`, `mlua`, or WASM plugins | Safe user extensions |

---

## 6. Platform Strategy

### 6.1 Desktop Shell: Tauri vs. Native

**Phase 1 (Tauri):** Web-based UI (React/Vue/Svelte) + Rust backend. Fast UI iteration, native feel. Binary ~600KB vs Electron's 150MB.

**Phase 2 (Custom winit+wgpu):** If Tauri's IPC overhead becomes a bottleneck for 60fps viewport interaction, build custom Rust UI using `egui` or `iced` directly in the viewport.

### 6.2 Web Deployment: WebAssembly + WebGPU

- Compile core to `wasm32-unknown-unknown` with `wasm-bindgen`
- **WebGPU** backend via `wgpu` (already abstracts WebGPU API)
- **Challenge:** Large DWG files in browser. **Solution:** Stream geometry using `ReadableStream` + incremental parsing
- **Performance:** Rust WASM runs at ~80-95% native speed for compute-heavy geometry

---

## 7. Technical Deep Dives

### 7.1 Document Model: CRDTs for CAD

AutoCAD's biggest collaboration failure is file locking. Forge uses **Operation-based CRDTs** or **Delta-state CRDTs**:

- Each entity has a UUID
- Operations: `AddEntity`, `DeleteEntity`, `ModifyProperty`, `Transform`
- Automatic conflict resolution for non-overlapping edits
- Offline-first: work locally, sync on reconnect

**Rust crates:** `automerge-rs`, or implement custom on `serde_json`/`ipld`

### 7.2 Geometry Kernel: Build vs. Bind

**Option A: Custom Rust Kernel** *(Recommended for 2D)*
- Lines, arcs, polylines, splines, NURBS
- Boolean operations via `clipper-rs` (2D) or custom BSP (3D)
- Constraint solving: geometric constraint networks (distance, angle, parallel, tangent)

**Option B: Bind to OpenCASCADE**
- `opencascade-rs` exists but adds C++ dependency
- Use only if 3D BRep solids are required immediately

**Option C: Hybrid**
- 2D pure Rust; 3D via STEP/IGES exchange to external mesher

### 7.3 Rendering Pipeline

```
Scene Graph (ECS) → Culling (R-Tree) → WGPU Render Pass → Output
```

- **2D:** Orthographic camera, line rendering with miter joins, hatch patterns via compute shaders
- **3D:** Phong/PBR shading, instanced rendering for blocks, LOD for large models
- **Text:** `cosmic-text` or `swash` for layout, `glyphon` for wgpu text rendering
- **Selection:** GPU picking (color-encoded entity IDs in framebuffer) or CPU raycasting

### 7.4 File I/O Strategy

| Format | Strategy |
|--------|----------|
| **Native (.forge)** | `rkyv` zero-copy archive + zstd compression; schema-versioned |
| **DXF** | Full Rust parser (well-documented ASCII/binary spec) |
| **DWG** | ODA File Converter integration, or reverse-engineer via `libredwg` concepts |
| **STEP/IFC** | Industry interoperability; `stepcode` Rust port or C bindings |
| **SVG/PDF** | Export for documentation; `resvg`, `printpdf` |

---

## 8. Development Roadmap

### Phase 1: The Viewport *(Months 1-3)*
- [ ] `winit` + `wgpu` window with pan/zoom/rotate
- [ ] ECS entity system (`hecs`)
- [ ] Draw line, circle, arc, polyline
- [ ] Layer palette
- [ ] Command parser (`nom`) + command line UI

### Phase 2: Editing *(Months 4-6)*
- [ ] Snapping (endpoint, midpoint, perpendicular, tangent)
- [ ] Modify commands: trim, extend, fillet, chamfer, offset
- [ ] Property panel (`egui`)
- [ ] Undo/redo (command pattern with CRDT ops)
- [ ] DXF import/export

### Phase 3: Productivity *(Months 7-9)*
- [ ] Block creation/insertion
- [ ] Hatch patterns (shader-based)
- [ ] Dimensioning (linear, angular, radial)
- [ ] Text and annotations
- [ ] Print/layout system

### Phase 4: Collaboration & Extensibility *(Months 10-12)*
- [ ] CRDT sync server (Rust + WebSockets)
- [ ] WASM plugin API
- [ ] Web deployment (WASM)
- [ ] Tauri desktop packaging

---

## 9. Risky Assumptions & Mitigations

| Risk | Mitigation |
|------|-----------|
| **DWG is proprietary** | Lead with DXF; offer DWG via ODA converter bridge; lobby for open CAD formats |
| **Rust GUI is immature** | Use `egui` for CAD panels (proven in game tools); web UI for chrome |
| **WebGPU adoption** | Fallback to WebGL2 via `wgpu`; desktop uses Vulkan/Metal natively |
| **3D kernel complexity** | Defer advanced solids; focus on 2D drafting + mesh 3D first |
| **Performance on web** | Streaming + LOD; don't load 100MB DWGs in browser |

---

## 10. Competitive Moat

1. **Real-time collaboration** — AutoCAD can't do this natively
2. **Native performance in browser** — WASM + WebGPU beats WebGL CAD apps
3. **Plugin safety** — WASM sandbox vs AutoCAD's .NET macro security nightmares
4. **Price** — Free tier undercuts $1,700/year AutoCAD subscription
5. **Open core** — Community contributes to the Rust geometry kernel

---

## 11. Conclusion

**Bottom line:** This is viable. The hardest part isn't the tech — it's the **geometry kernel maturity** and **DWG compatibility**. Start with 2D architectural drafting (where AutoCAD is overkill) and expand into mechanical 3D. Rust's ownership model actually *helps* with CAD's complex graph structures, preventing use-after-free in entity hierarchies.

---

*Generated for Forge brainstorming session — 2026-05-20*
