// Circle/Arc entity vertex/fragment shaders.
//
// Renders 2D circles and arcs as coloured line strips. The vertex shader
// transforms world-space positions through the camera view-projection
// matrix. The fragment shader passes through per-vertex colours.
//
// The Rust-side pipeline uses `LineStrip` topology for both circles
// and arcs (vertex format is identical to lines: position at location 0,
// colour at location 1).

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
