// Line entity vertex/fragment shaders.
//
// Renders 2D line segments as a coloured line list. The vertex shader
// transforms world-space positions through the camera view-projection
// matrix. The fragment shader passes through per-vertex colours.
//
// The Rust-side pipeline uses `LineList` topology.
// Vertex format: position (vec2<f32>) at location 0, colour (vec4<f32>)
// at location 1.

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> view_proj: mat4x4<f32>;

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = view_proj * vec4<f32>(in.position, 0.0, 1.0);
    out.color = in.color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
