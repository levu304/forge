// Snap marker vertex/fragment shaders.
//
// Renders screen-space billboard markers at the snapped world point.
// The vertex shader receives pixel offsets and the snap world position
// (via a uniform in group(1)), and transforms to clip space using
// the shared camera view-projection matrix (group(0)).
//
// Vertex format: offset (vec2<f32>) at location 0 — pixel offsets from
// the snap point in screen pixels, converted to world offset by
// multiplying with inv_zoom (snap_uniform.z).
//
// Bind groups:
//   @group(0) @binding(0) — camera view_proj matrix (shared with all renderers)
//   @group(1) @binding(0) — snap uniform: (snap_x, snap_y, inv_zoom, 0)

struct VertexInput {
    @location(0) offset: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> view_proj: mat4x4<f32>;

@group(1) @binding(0)
var<uniform> snap_uniform: vec4<f32>;

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    // Convert screen-pixel offset to a world-space offset by scaling
    // with inv_zoom (snap_uniform.z), then add to the snap world position.
    let world_pos = vec4<f32>(
        snap_uniform.x + in.offset.x * snap_uniform.z,
        snap_uniform.y + in.offset.y * snap_uniform.z,
        0.0,
        1.0,
    );
    out.clip_position = view_proj * world_pos;
    return out;
}

@fragment
fn fs_main() -> @location(0) vec4<f32> {
    // Yellow marker (rgba 255, 255, 0, 255).
    return vec4<f32>(1.0, 1.0, 0.0, 1.0);
}
