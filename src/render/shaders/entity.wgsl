// Generic entity vertex/fragment shaders.
//
// Used by both LineList (lines, polylines) and LineStrip (circles, arcs)
// pipelines — the topology is set in the Rust-side pipeline descriptor.
//
// Vertex format:
//   @location(0) position:    vec2<f32>  (offset  0,  8 bytes)
//   @location(1) color:       vec4<f32>  (offset  8, 16 bytes)
//   @location(2) is_selected: u32        (offset 24,  4 bytes)
// stride = 28 bytes
//
// When `is_selected == 1u`, the fragment shader blends a blue tint
// (`mix(color, selection_blue, 0.3)`) to indicate the entity is selected.

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) is_selected: u32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) @interpolate(flat) is_selected: u32,
};

@group(0) @binding(0)
var<uniform> view_proj: mat4x4<f32>;

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = view_proj * vec4<f32>(in.position, 0.0, 1.0);
    out.color = in.color;
    out.is_selected = in.is_selected;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    var color = in.color;
    if in.is_selected == 1u {
        // Selection blue tint: mix with rgb(0.0, 0.588, 1.0) at 30%
        color = mix(color, vec4<f32>(0.0, 0.588, 1.0, 1.0), 0.3);
    }
    color.a = 1.0; // v0.2.0: all entities are opaque (no transparency/layer system)
    return color;
}
