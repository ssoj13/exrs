//! Software 3D renderer for heightfield/pointcloud visualization.

use egui::{Color32, Painter, Pos2, Rect, Stroke};

/// 3D point.
#[derive(Clone, Copy, Debug)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    pub fn dot(self, other: Self) -> f32 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    pub fn cross(self, other: Self) -> Self {
        Self {
            x: self.y * other.z - self.z * other.y,
            y: self.z * other.x - self.x * other.z,
            z: self.x * other.y - self.y * other.x,
        }
    }

    pub fn normalize(self) -> Self {
        let len = (self.x * self.x + self.y * self.y + self.z * self.z).sqrt();
        if len > 1e-6 {
            Self {
                x: self.x / len,
                y: self.y / len,
                z: self.z / len,
            }
        } else {
            self
        }
    }

    pub fn sub(self, other: Self) -> Self {
        Self {
            x: self.x - other.x,
            y: self.y - other.y,
            z: self.z - other.z,
        }
    }
}

/// Camera for 3D view.
pub struct Camera {
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub target: Vec3,
    pub fov: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            yaw: 0.4,
            pitch: 0.5,
            distance: 2.0,
            target: Vec3::new(0.0, 0.0, 0.0),
            fov: 60.0,
        }
    }
}

impl Camera {
    /// Get camera position from spherical coords.
    pub fn position(&self) -> Vec3 {
        let x = self.distance * self.pitch.cos() * self.yaw.sin();
        let y = self.distance * self.pitch.sin();
        let z = self.distance * self.pitch.cos() * self.yaw.cos();
        Vec3::new(
            self.target.x + x,
            self.target.y + y,
            self.target.z + z,
        )
    }

    /// Project 3D point to 2D screen coords.
    pub fn project(&self, point: Vec3, viewport: Rect) -> Option<(Pos2, f32)> {
        let pos = self.position();
        let forward = Vec3::new(
            self.target.x - pos.x,
            self.target.y - pos.y,
            self.target.z - pos.z,
        )
        .normalize();
        let right = forward.cross(Vec3::new(0.0, 1.0, 0.0)).normalize();
        let up = right.cross(forward).normalize();

        // View space
        let rel = point.sub(pos);
        let vx = rel.dot(right);
        let vy = rel.dot(up);
        let vz = rel.dot(forward);

        // Behind camera
        if vz < 0.01 {
            return None;
        }

        // Perspective projection
        let aspect = viewport.width() / viewport.height();
        let fov_rad = self.fov.to_radians();
        let scale = (fov_rad / 2.0).tan();

        let px = vx / (vz * scale * aspect);
        let py = vy / (vz * scale);

        // NDC to screen
        let sx = viewport.center().x + px * viewport.width() / 2.0;
        let sy = viewport.center().y - py * viewport.height() / 2.0;

        // Depth for sorting/sizing
        let depth = vz;

        Some((Pos2::new(sx, sy), depth))
    }
}

/// Heightfield data for 3D rendering.
pub struct Heightfield {
    pub width: usize,
    pub height: usize,
    pub data: Vec<f32>,      // Z values
    pub colors: Vec<Color32>, // RGB colors per vertex
    pub min_z: f32,
    pub max_z: f32,
}

impl Heightfield {
    /// Create from depth data.
    pub fn from_depth(width: usize, height: usize, depth: &[f32], colors: Option<&[Color32]>) -> Self {
        let mut min_z = f32::MAX;
        let mut max_z = f32::MIN;
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

        let default_colors: Vec<Color32> = depth
            .iter()
            .map(|&z| {
                let t = ((z - min_z) / (max_z - min_z)).clamp(0.0, 1.0);
                depth_to_color(t)
            })
            .collect();

        Self {
            width,
            height,
            data: depth.to_vec(),
            colors: colors.map(|c| c.to_vec()).unwrap_or(default_colors),
            min_z,
            max_z,
        }
    }

    /// Get normalized Z at pixel.
    fn z_norm(&self, x: usize, y: usize) -> f32 {
        let idx = y * self.width + x;
        let z = self.data.get(idx).copied().unwrap_or(0.0);
        if self.max_z > self.min_z {
            (z - self.min_z) / (self.max_z - self.min_z)
        } else {
            0.0
        }
    }

    /// Get 3D position for pixel (normalized -0.5..0.5 XY, 0..height_scale Z).
    fn vertex(&self, x: usize, y: usize, height_scale: f32) -> Vec3 {
        let fx = (x as f32 / self.width as f32) - 0.5;
        let fy = (y as f32 / self.height as f32) - 0.5;
        let fz = self.z_norm(x, y) * height_scale;
        Vec3::new(fx, fz, fy)
    }

    /// Get color at pixel.
    fn color(&self, x: usize, y: usize) -> Color32 {
        let idx = y * self.width + x;
        self.colors.get(idx).copied().unwrap_or(Color32::GRAY)
    }
}

/// Point cloud data.
pub struct PointCloud {
    pub points: Vec<Vec3>,
    pub colors: Vec<Color32>,
}

impl PointCloud {
    /// Create from position pass (P.x, P.y, P.z channels).
    pub fn from_position_pass(
        width: usize,
        height: usize,
        px: &[f32],
        py: &[f32],
        pz: &[f32],
        colors: Option<&[Color32]>,
    ) -> Self {
        let mut points = Vec::with_capacity(width * height);
        let mut point_colors = Vec::with_capacity(width * height);

        // Find bounds for normalization
        let mut min = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
        let mut max = Vec3::new(f32::MIN, f32::MIN, f32::MIN);

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

        // Normalize to -0.5..0.5 range
        let scale_x = if max.x > min.x { max.x - min.x } else { 1.0 };
        let scale_y = if max.y > min.y { max.y - min.y } else { 1.0 };
        let scale_z = if max.z > min.z { max.z - min.z } else { 1.0 };
        let scale = scale_x.max(scale_y).max(scale_z);

        for i in 0..(width * height) {
            let x = px.get(i).copied().unwrap_or(0.0);
            let y = py.get(i).copied().unwrap_or(0.0);
            let z = pz.get(i).copied().unwrap_or(0.0);

            if x.is_finite() && y.is_finite() && z.is_finite() {
                let nx = (x - (min.x + max.x) / 2.0) / scale;
                let ny = (y - (min.y + max.y) / 2.0) / scale;
                let nz = (z - (min.z + max.z) / 2.0) / scale;
                points.push(Vec3::new(nx, ny, nz));

                let c = colors
                    .and_then(|cs| cs.get(i).copied())
                    .unwrap_or(Color32::WHITE);
                point_colors.push(c);
            }
        }

        Self {
            points,
            colors: point_colors,
        }
    }

    /// Create from depth (as point cloud, not mesh).
    pub fn from_depth(width: usize, height: usize, depth: &[f32], colors: Option<&[Color32]>) -> Self {
        let hf = Heightfield::from_depth(width, height, depth, colors);
        let mut points = Vec::with_capacity(width * height);
        let mut point_colors = Vec::with_capacity(width * height);

        for y in 0..height {
            for x in 0..width {
                points.push(hf.vertex(x, y, 0.3));
                point_colors.push(hf.color(x, y));
            }
        }

        Self {
            points,
            colors: point_colors,
        }
    }
}

/// Render heightfield as wireframe.
pub fn render_heightfield_wireframe(
    painter: &Painter,
    rect: Rect,
    camera: &Camera,
    heightfield: &Heightfield,
    step: usize,
    height_scale: f32,
) {
    let w = heightfield.width;
    let h = heightfield.height;
    let step = step.max(1);

    // Collect edges with depth for sorting
    let mut edges: Vec<(Pos2, Pos2, f32, Color32)> = Vec::new();

    for y in (0..h).step_by(step) {
        for x in (0..w).step_by(step) {
            let v0 = heightfield.vertex(x, y, height_scale);
            let c0 = heightfield.color(x, y);

            if let Some((p0, d0)) = camera.project(v0, rect) {
                // Right edge
                if x + step < w {
                    let v1 = heightfield.vertex(x + step, y, height_scale);
                    if let Some((p1, d1)) = camera.project(v1, rect) {
                        edges.push((p0, p1, (d0 + d1) / 2.0, c0));
                    }
                }
                // Down edge
                if y + step < h {
                    let v1 = heightfield.vertex(x, y + step, height_scale);
                    if let Some((p1, d1)) = camera.project(v1, rect) {
                        edges.push((p0, p1, (d0 + d1) / 2.0, c0));
                    }
                }
            }
        }
    }

    // Sort back to front
    edges.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

    // Draw
    for (p0, p1, depth, color) in edges {
        // Fade with distance
        let alpha = (1.0 - (depth / camera.distance).min(1.0)) * 0.8 + 0.2;
        let c = Color32::from_rgba_unmultiplied(
            color.r(),
            color.g(),
            color.b(),
            (alpha * 255.0) as u8,
        );
        painter.line_segment([p0, p1], Stroke::new(1.0, c));
    }
}

/// Render heightfield as solid (filled quads).
pub fn render_heightfield_solid(
    painter: &Painter,
    rect: Rect,
    camera: &Camera,
    heightfield: &Heightfield,
    step: usize,
    height_scale: f32,
) {
    let w = heightfield.width;
    let h = heightfield.height;
    let step = step.max(1);

    // Collect quads with depth
    let mut quads: Vec<([Pos2; 4], f32, Color32)> = Vec::new();

    for y in (0..h - step).step_by(step) {
        for x in (0..w - step).step_by(step) {
            let v00 = heightfield.vertex(x, y, height_scale);
            let v10 = heightfield.vertex(x + step, y, height_scale);
            let v01 = heightfield.vertex(x, y + step, height_scale);
            let v11 = heightfield.vertex(x + step, y + step, height_scale);

            let p00 = camera.project(v00, rect);
            let p10 = camera.project(v10, rect);
            let p01 = camera.project(v01, rect);
            let p11 = camera.project(v11, rect);

            if let (Some((p0, d0)), Some((p1, d1)), Some((p2, d2)), Some((p3, d3))) =
                (p00, p10, p01, p11)
            {
                let avg_depth = (d0 + d1 + d2 + d3) / 4.0;
                let c = heightfield.color(x, y);
                quads.push(([p0, p1, p3, p2], avg_depth, c));
            }
        }
    }

    // Sort back to front
    quads.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    // Draw filled quads
    for (pts, depth, color) in quads {
        // Simple shading based on depth
        let shade = (1.0 - (depth / (camera.distance * 2.0)).min(1.0)) * 0.6 + 0.4;
        let c = Color32::from_rgb(
            (color.r() as f32 * shade) as u8,
            (color.g() as f32 * shade) as u8,
            (color.b() as f32 * shade) as u8,
        );

        // Draw as two triangles
        painter.add(egui::Shape::convex_polygon(
            pts.to_vec(),
            c,
            Stroke::NONE,
        ));
    }
}

/// Render point cloud.
pub fn render_point_cloud(
    painter: &Painter,
    rect: Rect,
    camera: &Camera,
    cloud: &PointCloud,
    point_size: f32,
    max_points: usize,
) {
    // Subsample if too many points
    let step = (cloud.points.len() / max_points).max(1);

    let mut points: Vec<(Pos2, f32, Color32)> = Vec::new();

    for (i, (&pos, &color)) in cloud.points.iter().zip(cloud.colors.iter()).enumerate() {
        if i % step != 0 {
            continue;
        }
        if let Some((screen_pos, depth)) = camera.project(pos, rect) {
            points.push((screen_pos, depth, color));
        }
    }

    // Sort back to front
    points.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    // Draw points
    for (pos, depth, color) in points {
        // Size decreases with distance
        let size = (point_size * camera.distance / depth).clamp(1.0, point_size * 2.0);
        painter.circle_filled(pos, size, color);
    }
}

/// Depth value to color (blue-cyan-green-yellow-red).
fn depth_to_color(t: f32) -> Color32 {
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
    Color32::from_rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
}
