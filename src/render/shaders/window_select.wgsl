// Window-select rectangle vertex/fragment shaders.
//
// Renders a translucent filled quad with a border for window-selection
// feedback.  The quad is drawn in world-space coordinates with per-vertex
// colour (fill alpha ≈ 0.15, border alpha ≈ 0.8).
//
// Vertex format:
//   @location(0) position: vec2<f32>  (offset 0, 8 bytes)
//   @location(1) color:    vec4<f32>  (offset 8, 16 bytes)
// stride = 24 bytes

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
