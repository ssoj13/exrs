# 3D Viewer Implementation Plan

## Dependencies

```toml
[target.'cfg(feature = "view-3d")'.dependencies]
three-d = "0.17"
```

three-d уже включает egui интеграцию через `three_d::GUI`.

## Architecture

```
src/view/
├── app.rs          # Main app, ViewMode::View3D delegates to view3d
├── view3d.rs       # 3D scene setup, rendering, input
└── geometry.rs     # EXR → Mesh/PointCloud conversion
```

## View Modes

| Mode | Input | Output |
|------|-------|--------|
| Heightfield | Z channel | Grid mesh, Z = depth |
| PointCloud | Z or P.xyz | Points in 3D |
| PositionPass | P.x, P.y, P.z | Reconstructed geometry |

## three-d Integration

```rust
use three_d::*;

pub struct View3D {
    context: Context,
    camera: Camera,
    control: OrbitControl,
    
    // Geometry
    mesh: Option<Gm<Mesh, PhysicalMaterial>>,
    points: Option<Gm<PointCloud, PointCloudMaterial>>,
    
    // Helpers
    axes: Axes,
    grid: Option<Gm<Mesh, ColorMaterial>>,
}

impl View3D {
    pub fn new(context: &Context) -> Self {
        let camera = Camera::new_perspective(
            Viewport::new_at_origo(1, 1),
            vec3(0.0, 2.0, 4.0),  // position
            vec3(0.0, 0.0, 0.0),  // target
            vec3(0.0, 1.0, 0.0),  // up
            degrees(45.0),        // fov
            0.1, 100.0,           // near, far
        );
        
        let control = OrbitControl::new(
            camera.target(),
            1.0,  // min distance
            100.0 // max distance
        );
        
        Self { context, camera, control, mesh: None, points: None, axes: Axes::new(&context, 0.1, 1.0), grid: None }
    }
    
    pub fn handle_events(&mut self, frame_input: &FrameInput) {
        self.control.handle_events(&mut self.camera, &frame_input.events);
    }
    
    pub fn render(&self, target: &RenderTarget) {
        let objects: Vec<&dyn Object> = vec![&self.axes];
        // + mesh/points if present
        target.clear(ClearState::color_and_depth(0.1, 0.1, 0.1, 1.0, 1.0));
        target.render(&self.camera, objects, &[]);
    }
}
```

## Geometry Generation

```rust
// Heightfield from depth channel
pub fn heightfield_from_channel(
    pixels: &[f32],
    width: usize,
    height: usize,
    scale: f32,
) -> CpuMesh {
    let mut positions = Vec::with_capacity(width * height * 3);
    let mut indices = Vec::new();
    
    for y in 0..height {
        for x in 0..width {
            let z = pixels[y * width + x] * scale;
            positions.push(vec3(x as f32, z, y as f32));
        }
    }
    
    // Grid triangulation
    for y in 0..(height - 1) {
        for x in 0..(width - 1) {
            let i = (y * width + x) as u32;
            indices.push(i);
            indices.push(i + 1);
            indices.push(i + width as u32);
            // second triangle...
        }
    }
    
    CpuMesh { positions, indices, .. }
}

// PointCloud from P channels
pub fn pointcloud_from_position(
    px: &[f32], py: &[f32], pz: &[f32],
    width: usize, height: usize,
) -> Vec<Vec3> {
    (0..width*height)
        .map(|i| vec3(px[i], py[i], pz[i]))
        .filter(|p| p.magnitude() < 1e6)  // skip invalid
        .collect()
}
```

## Controls

| Input | Action |
|-------|--------|
| LMB drag | Orbit |
| RMB drag | Pan |
| Scroll | Zoom |
| F | Fit to bounds |
| 1/2/3 | Heightfield/PointCloud/PositionPass |
| G | Toggle grid |
| W | Toggle wireframe |

## Implementation Steps

### Phase 1: three-d Setup
1. [ ] Add three-d dependency
2. [ ] Create View3D struct with Context, Camera, OrbitControl
3. [ ] Render loop integration with egui
4. [ ] Basic axes + grid

### Phase 2: Heightfield
1. [ ] Channel → CpuMesh conversion
2. [ ] Depth-based vertex colors (heatmap)
3. [ ] PhysicalMaterial with flat shading
4. [ ] Scale/offset controls

### Phase 3: PointCloud
1. [ ] PointCloud geometry
2. [ ] Point size control
3. [ ] Color by depth or channel value

### Phase 4: Position Pass
1. [ ] Auto-detect P.x/P.y/P.z channels
2. [ ] Build geometry from P channels
3. [ ] Optional mesh reconstruction

## egui + three-d

three-d рендерит в текстуру, egui показывает её как Image:

```rust
// В app.rs
fn draw_3d_canvas(&mut self, ui: &mut egui::Ui, available: Vec2) {
    let size = available.as_uvec2();
    
    // Resize render target if needed
    self.view3d.resize(size.x, size.y);
    
    // Handle input
    let response = ui.allocate_response(available, egui::Sense::drag());
    self.view3d.handle_input(&response, ui);
    
    // Render to texture
    self.view3d.render();
    
    // Show texture in egui
    let texture_id = self.view3d.egui_texture_id();
    ui.image(texture_id, available);
}
```

## Test Files

Нужны EXR с:
- Z/depth channel
- P.x, P.y, P.z (position pass)

Можно сгенерить через `exrs gen` или экспорт из Blender.
