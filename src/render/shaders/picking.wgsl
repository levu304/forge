// Picking pass shader.
//
// Renders entity instance indices as flat u32 colors into an offscreen
// Rgba32Uint framebuffer. The CPU reads back the pixel under the cursor
// to determine which entity (if any) was clicked.
//
// The vertex shader passes @builtin(instance_index) through as a flat
// u32 to the fragment shader, which writes it as the red channel of
// the output. A sentinel value of 0xFFFFFFFF means "no entity".
//
// Vertex format: position (vec2<f32>) at location 0 only.
// Stride = 8 bytes (no colour attribute needed for picking).

struct VertexInput {
    @location(0) position: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) @interpolate(flat) entity_id: u32,
};

@group(0) @binding(0)
var<uniform> view_proj: mat4x4<f32>;

@vertex
fn vs_main(in: VertexInput, @builtin(instance_index) instance: u32) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = view_proj * vec4<f32>(in.position, 0.0, 1.0);
    out.entity_id = instance;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<u32> {
    return vec4<u32>(in.entity_id, 0u, 0u, 0xFFFFFFFFu);
}
