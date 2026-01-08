//! 3D viewer using three-d library.

#![allow(dead_code)]  // WIP: many methods not yet called

use std::sync::Arc;
use three_d::*;

/// 3D view mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode3D {
    #[default]
    Heightfield,
    PointCloud,
    PositionPass,
}

/// 3D viewer state and rendering.
pub struct View3D {
    context: Context,
    camera: Camera,
    control: OrbitControl,
    
    // Scene objects
    mesh: Option<Gm<Mesh, ColorMaterial>>,
    points: Option<Gm<InstancedMesh, ColorMaterial>>,
    axes: Axes,
    grid: Vec<Gm<Mesh, ColorMaterial>>,
    
    // State
    mode: Mode3D,
    show_grid: bool,
    wireframe: bool,
}

impl std::fmt::Debug for View3D {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("View3D")
            .field("mode", &self.mode)
            .field("show_grid", &self.show_grid)
            .field("wireframe", &self.wireframe)
            .finish_non_exhaustive()
    }
}

impl View3D {
    /// Create new 3D viewer from glow context.
    pub fn new(gl: Arc<eframe::glow::Context>) -> Self {
        let context = Context::from_gl_context(gl).expect("failed to create three-d context");
        
        let camera = Camera::new_perspective(
            Viewport::new_at_origo(800, 600),
            vec3(2.0, 1.5, 2.0),
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 1.0, 0.0),
            degrees(45.0),
            0.01,
            100.0,
        );
        
        let control = OrbitControl::new(
            vec3(0.0, 0.0, 0.0),
            0.1,
            50.0,
        );
        
        let axes = Axes::new(&context, 0.02, 0.5);
        let grid = Self::create_grid(&context);
        
        Self {
            context,
            camera,
            control,
            mesh: None,
            points: None,
            axes,
            grid,
            mode: Mode3D::Heightfield,
            show_grid: true,
            wireframe: false,
        }
    }
    
    /// Create grid as line segments.
    fn create_grid(context: &Context) -> Vec<Gm<Mesh, ColorMaterial>> {
        let mut lines = Vec::new();
        let size = 1.0;
        let divisions = 10;
        let step = size * 2.0 / divisions as f32;
        let color = Srgba::new(80, 80, 80, 255);
        
        // Create individual line segments as thin quads
        // This is a workaround since three-d doesn't have line primitives
        let line_width = 0.002;
        
        // Grid lines along X axis
        for i in 0..=divisions {
            let z = -size + i as f32 * step;
            let positions = vec![
                vec3(-size, 0.0, z - line_width),
                vec3(size, 0.0, z - line_width),
                vec3(size, 0.0, z + line_width),
                vec3(-size, 0.0, z + line_width),
            ];
            let indices = vec![0u32, 1, 2, 0, 2, 3];
            
            let cpu_mesh = CpuMesh {
                positions: Positions::F32(positions),
                indices: Indices::U32(indices),
                ..Default::default()
            };
            
            let mesh = Mesh::new(context, &cpu_mesh);
            lines.push(Gm::new(mesh, ColorMaterial {
                color,
                ..Default::default()
            }));
        }
        
        // Grid lines along Z axis
        for i in 0..=divisions {
            let x = -size + i as f32 * step;
            let positions = vec![
                vec3(x - line_width, 0.0, -size),
                vec3(x + line_width, 0.0, -size),
                vec3(x + line_width, 0.0, size),
                vec3(x - line_width, 0.0, size),
            ];
            let indices = vec![0u32, 1, 2, 0, 2, 3];
            
            let cpu_mesh = CpuMesh {
                positions: Positions::F32(positions),
                indices: Indices::U32(indices),
                ..Default::default()
            };
            
            let mesh = Mesh::new(context, &cpu_mesh);
            lines.push(Gm::new(mesh, ColorMaterial {
                color,
                ..Default::default()
            }));
        }
        
        lines
    }
    
    /// Set heightfield data from depth channel.
    pub fn set_heightfield(&mut self, width: usize, height: usize, depth: &[f32]) {
        // Find depth range
        let (mut min_z, mut max_z) = (f32::MAX, f32::MIN);
        for &z in depth {
            if z.is_finite() {
                min_z = min_z.min(z);
                max_z = max_z.max(z);
            }
        }
        if min_z >= max_z {
            min_z = 0.0;
            max_z = 1.0;
        }
        let range = max_z - min_z;
        
        // Downsample for performance
        let max_dim = 256;
        let step = ((width.max(height)) / max_dim).max(1);
        let w = width / step;
        let h = height / step;
        
        // Calculate aspect ratio to preserve image proportions
        let aspect = width as f32 / height as f32;
        let (scale_x, scale_z) = if aspect > 1.0 {
            (0.5, 0.5 / aspect)
        } else {
            (0.5 * aspect, 0.5)
        };
        
        // Generate vertices
        let mut positions = Vec::with_capacity(w * h);
        let mut colors = Vec::with_capacity(w * h);
        
        for y in 0..h {
            for x in 0..w {
                let src_x = x * step;
                let src_y = y * step;
                let idx = src_y * width + src_x;
                let z = depth.get(idx).copied().unwrap_or(0.0);
                
                // Normalize coords with aspect ratio
                let fx = (x as f32 / w as f32 - 0.5) * 2.0 * scale_x;
                let fz = (y as f32 / h as f32 - 0.5) * 2.0 * scale_z;
                let fy = if z.is_finite() {
                    ((z - min_z) / range) * 0.3
                } else {
                    0.0
                };
                
                positions.push(vec3(fx, fy, fz));
                
                // Color by height (blue->cyan->green->yellow->red)
                let t = fy / 0.3;
                let color = depth_to_color(t);
                colors.push(color);
            }
        }
        
        // Generate indices for triangles
        let mut indices = Vec::new();
        for y in 0..(h - 1) {
            for x in 0..(w - 1) {
                let i = (y * w + x) as u32;
                // First triangle
                indices.push(i);
                indices.push(i + 1);
                indices.push(i + w as u32);
                // Second triangle
                indices.push(i + 1);
                indices.push(i + w as u32 + 1);
                indices.push(i + w as u32);
            }
        }
        
        let cpu_mesh = CpuMesh {
            positions: Positions::F32(positions),
            colors: Some(colors),
            indices: Indices::U32(indices),
            ..Default::default()
        };
        
        let mesh = Mesh::new(&self.context, &cpu_mesh);
        self.mesh = Some(Gm::new(mesh, ColorMaterial::default()));
        self.points = None;
        self.mode = Mode3D::Heightfield;
    }
    
    /// Set point cloud from depth channel.
    pub fn set_pointcloud(&mut self, width: usize, height: usize, depth: &[f32]) {
        // Find depth range
        let (mut min_z, mut max_z) = (f32::MAX, f32::MIN);
        for &z in depth {
            if z.is_finite() {
                min_z = min_z.min(z);
                max_z = max_z.max(z);
            }
        }
        if min_z >= max_z {
            min_z = 0.0;
            max_z = 1.0;
        }
        let range = max_z - min_z;
        
        // Subsample
        let max_points = 50000;
        let step = ((width * height) / max_points).max(1);
        
        // Calculate aspect ratio
        let aspect = width as f32 / height as f32;
        let (scale_x, scale_z) = if aspect > 1.0 {
            (0.5, 0.5 / aspect)
        } else {
            (0.5 * aspect, 0.5)
        };
        
        let mut transforms = Vec::new();
        let mut colors = Vec::new();
        
        for (i, &z) in depth.iter().enumerate() {
            if i % step != 0 || !z.is_finite() {
                continue;
            }
            
            let x = i % width;
            let y = i / width;
            
            let fx = (x as f32 / width as f32 - 0.5) * 2.0 * scale_x;
            let fz = (y as f32 / height as f32 - 0.5) * 2.0 * scale_z;
            let fy = ((z - min_z) / range) * 0.3;
            
            transforms.push(Mat4::from_translation(vec3(fx, fy, fz)) * Mat4::from_scale(0.002));
            
            let t = fy / 0.3;
            colors.push(depth_to_color(t));
        }
        
        if transforms.is_empty() {
            return;
        }
        
        // Create sphere instances
        let sphere = CpuMesh::sphere(4);
        let instances = Instances {
            transformations: transforms,
            colors: Some(colors),
            ..Default::default()
        };
        
        let instanced = InstancedMesh::new(&self.context, &instances, &sphere);
        self.points = Some(Gm::new(instanced, ColorMaterial::default()));
        self.mesh = None;
        self.mode = Mode3D::PointCloud;
    }
    
    /// Set position pass from P.xyz channels.
    pub fn set_position_pass(&mut self, width: usize, height: usize, px: &[f32], py: &[f32], pz: &[f32]) {
        // Find bounds
        let (mut min, mut max) = (
            vec3(f32::MAX, f32::MAX, f32::MAX),
            vec3(f32::MIN, f32::MIN, f32::MIN),
        );
        
        for i in 0..(width * height) {
            let x = px.get(i).copied().unwrap_or(0.0);
            let y = py.get(i).copied().unwrap_or(0.0);
            let z = pz.get(i).copied().unwrap_or(0.0);
            if x.is_finite() && y.is_finite() && z.is_finite() {
                min.x = min.x.min(x);
                min.y = min.y.min(y);
                min.z = min.z.min(z);
                max.x = max.x.max(x);
                max.y = max.y.max(y);
                max.z = max.z.max(z);
            }
        }
        
        let center = (min + max) * 0.5;
        let scale = (max.x - min.x).max(max.y - min.y).max(max.z - min.z).max(0.001);
        
        // Subsample
        let max_points = 50000;
        let step = ((width * height) / max_points).max(1);
        
        let mut transforms = Vec::new();
        let mut colors = Vec::new();
        
        for i in 0..(width * height) {
            if i % step != 0 {
                continue;
            }
            
            let x = px.get(i).copied().unwrap_or(0.0);
            let y = py.get(i).copied().unwrap_or(0.0);
            let z = pz.get(i).copied().unwrap_or(0.0);
            
            if !x.is_finite() || !y.is_finite() || !z.is_finite() {
                continue;
            }
            
            // Normalize to -0.5..0.5 range
            let nx = (x - center.x) / scale;
            let ny = (y - center.y) / scale;
            let nz = (z - center.z) / scale;
            
            transforms.push(Mat4::from_translation(vec3(nx, ny, nz)) * Mat4::from_scale(0.002));
            
            // Color by normalized Y
            let t = ny + 0.5;
            colors.push(depth_to_color(t));
        }
        
        if transforms.is_empty() {
            return;
        }
        
        let sphere = CpuMesh::sphere(4);
        let instances = Instances {
            transformations: transforms,
            colors: Some(colors),
            ..Default::default()
        };
        
        let instanced = InstancedMesh::new(&self.context, &instances, &sphere);
        self.points = Some(Gm::new(instanced, ColorMaterial::default()));
        self.mesh = None;
        self.mode = Mode3D::PositionPass;
    }
    
    /// Handle input events.
    pub fn handle_events(&mut self, events: &mut [Event]) -> bool {
        self.control.handle_events(&mut self.camera, events)
    }
    
    /// Toggle grid visibility.
    pub fn toggle_grid(&mut self) {
        self.show_grid = !self.show_grid;
    }
    
    /// Toggle wireframe mode.
    pub fn toggle_wireframe(&mut self) {
        self.wireframe = !self.wireframe;
    }
    
    /// Reset camera to default position.
    pub fn reset_camera(&mut self) {
        self.camera.set_view(
            vec3(2.0, 1.5, 2.0),
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 1.0, 0.0),
        );
        // Reset control by recreating it
        self.control = OrbitControl::new(
            vec3(0.0, 0.0, 0.0),
            0.1,
            50.0,
        );
    }
    
    /// Render the 3D scene.
    pub fn render(&self, viewport: Viewport) {
        let mut camera = self.camera.clone();
        camera.set_viewport(viewport);
        
        // Screen render target sized to fit our panel
        let screen = RenderTarget::screen(&self.context, viewport.width, viewport.height);
        
        // Scissor box for clear and render - restricts operations to our panel area
        let scissor = ScissorBox {
            x: viewport.x,
            y: viewport.y,
            width: viewport.width,
            height: viewport.height,
        };
        
        // Collect objects to render
        let mut objects: Vec<&dyn Object> = vec![&self.axes];
        
        if self.show_grid {
            for grid_line in &self.grid {
                objects.push(grid_line);
            }
        }
        
        if let Some(ref mesh) = self.mesh {
            objects.push(mesh);
        }
        
        if let Some(ref points) = self.points {
            objects.push(points);
        }
        
        let bg = Srgba::new(30, 30, 30, 255);
        
        // Clear and render only within scissor box
        screen
            .clear_partially(scissor, ClearState::color_and_depth(
                bg.r as f32 / 255.0,
                bg.g as f32 / 255.0,
                bg.b as f32 / 255.0,
                1.0,
                1.0,
            ))
            .render_partially(scissor, &camera, objects, &[]);
    }
    
    /// Get current mode.
    pub fn mode(&self) -> Mode3D {
        self.mode
    }
    
    /// Check if any 3D data is loaded.
    pub fn has_data(&self) -> bool {
        self.mesh.is_some() || self.points.is_some()
    }
}

/// Depth to color mapping (blue->cyan->green->yellow->red).
fn depth_to_color(t: f32) -> Srgba {
    let t = t.clamp(0.0, 1.0);
    let (r, g, b) = if t < 0.25 {
        let s = t / 0.25;
        (0.0, s, 1.0)
    } else if t < 0.5 {
        let s = (t - 0.25) / 0.25;
        (0.0, 1.0, 1.0 - s)
    } else if t < 0.75 {
        let s = (t - 0.5) / 0.25;
        (s, 1.0, 0.0)
    } else {
        let s = (t - 0.75) / 0.25;
        (1.0, 1.0 - s, 0.0)
    };
    Srgba::new(
        (r * 255.0) as u8,
        (g * 255.0) as u8,
        (b * 255.0) as u8,
        255,
    )
}
