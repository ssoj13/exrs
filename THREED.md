# 3D Viewer Implementation Plan

## Overview

EXR 3D viewer for depth/position data visualization with orbit camera controls.

## Dependencies

```toml
[dependencies]
# Option A: Full 3D engine integration
three-d = "0.17"           # GPU-accelerated 3D rendering
egui_render_three_d = "0.6" # egui + three-d bridge

# Option B: Software rasterizer (no GPU deps)
# Already have everything needed in render3d.rs
```

**Recommendation**: Start with Option B (software), upgrade to three-d if performance issues.

## Architecture

```
src/view/
├── app.rs          # Main app, delegates to render3d
├── render3d.rs     # 3D rendering logic
└── camera.rs       # Orbit camera (extract from render3d)
```

## View Modes

### 1. Heightfield (Z-buffer as mesh)
- Input: Single depth channel (Z, depth, or luminance)
- Output: Grid mesh where Z = pixel value
- Use case: Depth passes, displacement preview

### 2. PointCloud
- Input: Single channel (Z) or position (P.x, P.y, P.z)
- Output: Points in 3D space
- Use case: Sparse data, lidar-style viz

### 3. PositionPass (P channels)
- Input: P.x, P.y, P.z channels (world-space positions)
- Output: Reconstructed 3D geometry
- Use case: Position AOVs from renderers

## Camera System

```rust
pub struct OrbitCamera {
    pub target: Vec3,      // Look-at point
    pub distance: f32,     // Distance from target
    pub yaw: f32,          // Horizontal rotation (radians)
    pub pitch: f32,        // Vertical rotation (clamped)
    pub fov: f32,          // Field of view (degrees)
    pub near: f32,         // Near clip
    pub far: f32,          // Far clip
}

impl OrbitCamera {
    // Mouse controls
    fn rotate(&mut self, dx: f32, dy: f32);     // LMB drag
    fn pan(&mut self, dx: f32, dy: f32);        // MMB drag
    fn zoom(&mut self, delta: f32);             // Scroll wheel
    fn fit_to_bounds(&mut self, bounds: AABB);  // F key
    fn reset(&mut self);                        // Home key
    
    // Matrices
    fn view_matrix(&self) -> Mat4;
    fn proj_matrix(&self, aspect: f32) -> Mat4;
}
```

## Controls

| Input | Action |
|-------|--------|
| LMB drag | Orbit (rotate around target) |
| MMB drag | Pan (move target) |
| Scroll | Zoom (change distance) |
| F | Fit to bounds |
| Home | Reset camera |
| R | Toggle wireframe/solid |
| 1/2/3 | Switch Heightfield/PointCloud/PositionPass |

## Implementation Steps

### Phase 1: Camera & Basic Rendering
1. [ ] Extract `OrbitCamera` to `camera.rs`
2. [ ] Implement mouse interaction in egui
3. [ ] Software rasterizer for triangles
4. [ ] Depth buffer (painter's algorithm or z-buffer)

### Phase 2: Heightfield Mode
1. [ ] Generate grid mesh from depth channel
2. [ ] Vertex colors from depth (heatmap)
3. [ ] Wireframe rendering
4. [ ] Solid rendering with flat shading

### Phase 3: PointCloud Mode  
1. [ ] Point rendering (circles in screen space)
2. [ ] Point size control
3. [ ] Depth-based coloring
4. [ ] Optional: Point splatting

### Phase 4: Position Pass Mode
1. [ ] Detect P.x/P.y/P.z channels
2. [ ] Build point cloud from P channels
3. [ ] Optional: Triangle reconstruction (Delaunay or grid-based)

### Phase 5: Polish
1. [ ] Grid floor
2. [ ] Axis gizmo
3. [ ] Bounding box display
4. [ ] Scale/offset controls for depth
5. [ ] Export to OBJ/PLY

## Data Flow

```
EXR File
    │
    ▼
Channel Selection (UI dropdown)
    │
    ├─► Single channel (Z, depth, R, etc.)
    │       │
    │       ▼
    │   Heightfield or PointCloud
    │
    └─► P.x + P.y + P.z channels
            │
            ▼
        PositionPass mode
            │
            ▼
    Vec<Vec3> world positions
            │
            ▼
    Camera transform → Screen coords
            │
            ▼
    egui Painter (lines, circles, triangles)
```

## Software Renderer Core

```rust
// Triangle rasterization (scanline)
fn rasterize_triangle(
    painter: &Painter,
    v0: Vec3, v1: Vec3, v2: Vec3,  // Screen coords (z = depth)
    c0: Color32, c1: Color32, c2: Color32,
    zbuffer: &mut [f32],
    width: usize,
);

// Point rendering
fn render_point(
    painter: &Painter,
    pos: Vec3,        // Screen coords
    color: Color32,
    size: f32,
);

// Line rendering (already in egui)
fn render_line(
    painter: &Painter,
    p0: Pos2, p1: Pos2,
    color: Color32,
    width: f32,
);
```

## Performance Considerations

- **Downsampling**: For large images (4K+), downsample to ~512x512 for 3D
- **LOD**: Distance-based point/triangle culling
- **Frustum culling**: Skip off-screen geometry
- **Batching**: Collect all lines/triangles, draw in single call

## UI Integration

```rust
// In app.rs draw_3d_canvas()
fn draw_3d_canvas(&mut self, ui: &mut egui::Ui, available: Vec2) {
    let (rect, response) = ui.allocate_exact_size(available, egui::Sense::drag());
    
    // Handle input
    if response.dragged_by(PointerButton::Primary) {
        let delta = response.drag_delta();
        self.camera.rotate(delta.x * 0.01, delta.y * 0.01);
    }
    if response.dragged_by(PointerButton::Middle) {
        let delta = response.drag_delta();
        self.camera.pan(delta.x, delta.y);
    }
    if let Some(scroll) = ui.input(|i| i.scroll_delta.y) {
        self.camera.zoom(scroll * 0.1);
    }
    
    // Render
    let painter = ui.painter_at(rect);
    self.render_3d(&painter, rect);
}
```

## Timeline Estimate

- Phase 1: Camera + basics - foundation
- Phase 2: Heightfield - первый видимый результат  
- Phase 3: PointCloud - быстро после Phase 2
- Phase 4: Position pass - если есть P channels в тестовых файлах
- Phase 5: Polish - по желанию

## Test Data

Need EXR files with:
- [ ] Z/depth channel
- [ ] P.x, P.y, P.z channels (position pass)
- [ ] Deep data with varying samples

Can generate with `exrs::gen` module or export from Blender/Houdini.
