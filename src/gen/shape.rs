//! 3D shape generators for Z-depth channels.
//!
//! These generate depth values as if rendering simple 3D primitives.

use super::noise;
use super::pattern::norm;

/// Z-depth generator trait.
pub trait ZShape: Send + Sync {
    /// Generate depth at (x, y). Returns depth in [0, 1] where 0=near, 1=far.
    /// Returns None if pixel is outside the shape (background).
    fn depth(&self, x: usize, y: usize, width: usize, height: usize) -> Option<f32>;

    /// Shape name.
    fn name(&self) -> &'static str;

    /// Sample with background value for pixels outside shape.
    fn sample(&self, x: usize, y: usize, width: usize, height: usize, bg: f32) -> f32 {
        self.depth(x, y, width, height).unwrap_or(bg)
    }
}

// ============================================================================
// Basic primitives
// ============================================================================

/// Sphere - classic depth buffer shape.
#[derive(Clone, Copy, Debug)]
pub struct Sphere {
    pub cx: f32,
    pub cy: f32,
    pub radius: f32,
    pub near: f32,
    pub far: f32,
}

impl Default for Sphere {
    fn default() -> Self {
        Self { cx: 0.5, cy: 0.5, radius: 0.4, near: 0.3, far: 0.7 }
    }
}

impl ZShape for Sphere {
    fn depth(&self, x: usize, y: usize, width: usize, height: usize) -> Option<f32> {
        let nx = norm(x, width) - self.cx;
        let ny = norm(y, height) - self.cy;
        let r2 = nx * nx + ny * ny;
        let rad2 = self.radius * self.radius;

        if r2 > rad2 {
            return None;
        }

        // Sphere surface: z = sqrt(r^2 - x^2 - y^2)
        let z = (rad2 - r2).sqrt() / self.radius; // 0 at edge, 1 at center
        let depth = self.near + (1.0 - z) * (self.far - self.near);
        Some(depth)
    }

    fn name(&self) -> &'static str { "sphere" }
}

/// Box/Cube - flat front face with depth.
#[derive(Clone, Copy, Debug)]
pub struct Box {
    pub cx: f32,
    pub cy: f32,
    pub half_w: f32,
    pub half_h: f32,
    pub depth: f32,
}

impl Default for Box {
    fn default() -> Self {
        Self { cx: 0.5, cy: 0.5, half_w: 0.3, half_h: 0.3, depth: 0.5 }
    }
}

impl ZShape for Box {
    fn depth(&self, x: usize, y: usize, width: usize, height: usize) -> Option<f32> {
        let nx = (norm(x, width) - self.cx).abs();
        let ny = (norm(y, height) - self.cy).abs();

        if nx <= self.half_w && ny <= self.half_h {
            Some(self.depth)
        } else {
            None
        }
    }

    fn name(&self) -> &'static str { "box" }
}

/// Plane at angle - linear depth gradient.
#[derive(Clone, Copy, Debug)]
pub struct Plane {
    pub near: f32,
    pub far: f32,
    pub angle: f32, // 0 = horizontal gradient, 90 = vertical
}

impl Default for Plane {
    fn default() -> Self {
        Self { near: 0.2, far: 0.8, angle: 0.0 }
    }
}

impl ZShape for Plane {
    fn depth(&self, x: usize, y: usize, width: usize, height: usize) -> Option<f32> {
        let nx = norm(x, width);
        let ny = norm(y, height);
        let rad = self.angle.to_radians();
        let t = nx * rad.cos() + ny * rad.sin();
        let t = t / (rad.cos().abs() + rad.sin().abs()); // Normalize
        Some(self.near + t * (self.far - self.near))
    }

    fn name(&self) -> &'static str { "plane" }
}

/// Cone - radial depth gradient.
#[derive(Clone, Copy, Debug)]
pub struct Cone {
    pub cx: f32,
    pub cy: f32,
    pub radius: f32,
    pub near: f32,  // Depth at center
    pub far: f32,   // Depth at edge
}

impl Default for Cone {
    fn default() -> Self {
        Self { cx: 0.5, cy: 0.5, radius: 0.4, near: 0.2, far: 0.8 }
    }
}

impl ZShape for Cone {
    fn depth(&self, x: usize, y: usize, width: usize, height: usize) -> Option<f32> {
        let nx = norm(x, width) - self.cx;
        let ny = norm(y, height) - self.cy;
        let dist = (nx * nx + ny * ny).sqrt();

        if dist > self.radius {
            return None;
        }

        let t = dist / self.radius;
        Some(self.near + t * (self.far - self.near))
    }

    fn name(&self) -> &'static str { "cone" }
}

/// Cylinder - flat top with circular outline.
#[derive(Clone, Copy, Debug)]
pub struct Cylinder {
    pub cx: f32,
    pub cy: f32,
    pub radius: f32,
    pub depth: f32,
}

impl Default for Cylinder {
    fn default() -> Self {
        Self { cx: 0.5, cy: 0.5, radius: 0.35, depth: 0.5 }
    }
}

impl ZShape for Cylinder {
    fn depth(&self, x: usize, y: usize, width: usize, height: usize) -> Option<f32> {
        let nx = norm(x, width) - self.cx;
        let ny = norm(y, height) - self.cy;
        let dist = (nx * nx + ny * ny).sqrt();

        if dist <= self.radius {
            Some(self.depth)
        } else {
            None
        }
    }

    fn name(&self) -> &'static str { "cylinder" }
}

/// Torus - donut shape.
#[derive(Clone, Copy, Debug)]
pub struct Torus {
    pub cx: f32,
    pub cy: f32,
    pub major_radius: f32,
    pub minor_radius: f32,
    pub near: f32,
    pub far: f32,
}

impl Default for Torus {
    fn default() -> Self {
        Self {
            cx: 0.5, cy: 0.5,
            major_radius: 0.3,
            minor_radius: 0.1,
            near: 0.3, far: 0.7,
        }
    }
}

impl ZShape for Torus {
    fn depth(&self, x: usize, y: usize, width: usize, height: usize) -> Option<f32> {
        let nx = norm(x, width) - self.cx;
        let ny = norm(y, height) - self.cy;
        let dist = (nx * nx + ny * ny).sqrt();

        // Distance from the ring center
        let ring_dist = (dist - self.major_radius).abs();
        if ring_dist > self.minor_radius {
            return None;
        }

        // Height on minor circle
        let z = (self.minor_radius * self.minor_radius - ring_dist * ring_dist).sqrt();
        let z_norm = z / self.minor_radius;
        Some(self.near + (1.0 - z_norm) * (self.far - self.near))
    }

    fn name(&self) -> &'static str { "torus" }
}

// ============================================================================
// Procedural terrain
// ============================================================================

/// Perlin terrain - smooth hills.
#[derive(Clone, Copy, Debug)]
pub struct Terrain {
    pub frequency: f32,
    pub octaves: u32,
    pub amplitude: f32,
    pub base: f32,
    pub seed: u32,
}

impl Default for Terrain {
    fn default() -> Self {
        Self { frequency: 4.0, octaves: 4, amplitude: 0.4, base: 0.5, seed: 42 }
    }
}

impl ZShape for Terrain {
    fn depth(&self, x: usize, y: usize, width: usize, height: usize) -> Option<f32> {
        let nx = norm(x, width) * self.frequency;
        let ny = norm(y, height) * self.frequency;
        let n = noise::fbm(nx, ny, self.octaves, self.seed);
        Some(self.base + (n - 0.5) * self.amplitude)
    }

    fn name(&self) -> &'static str { "terrain" }
}

/// Ridged mountains.
#[derive(Clone, Copy, Debug)]
pub struct Mountains {
    pub frequency: f32,
    pub octaves: u32,
    pub amplitude: f32,
    pub base: f32,
    pub seed: u32,
}

impl Default for Mountains {
    fn default() -> Self {
        Self { frequency: 3.0, octaves: 5, amplitude: 0.5, base: 0.3, seed: 42 }
    }
}

impl ZShape for Mountains {
    fn depth(&self, x: usize, y: usize, width: usize, height: usize) -> Option<f32> {
        let nx = norm(x, width) * self.frequency;
        let ny = norm(y, height) * self.frequency;
        let n = noise::ridged(nx, ny, self.octaves, self.seed);
        Some(self.base + n * self.amplitude)
    }

    fn name(&self) -> &'static str { "mountains" }
}

/// Wave surface.
#[derive(Clone, Copy, Debug)]
pub struct WaveSurface {
    pub freq_x: f32,
    pub freq_y: f32,
    pub amplitude: f32,
    pub base: f32,
    pub phase: f32,
}

impl Default for WaveSurface {
    fn default() -> Self {
        Self { freq_x: 4.0, freq_y: 4.0, amplitude: 0.2, base: 0.5, phase: 0.0 }
    }
}

impl ZShape for WaveSurface {
    fn depth(&self, x: usize, y: usize, width: usize, height: usize) -> Option<f32> {
        let nx = norm(x, width);
        let ny = norm(y, height);
        let n = noise::waves(nx, ny, self.freq_x, self.freq_y, self.phase);
        Some(self.base + (n - 0.5) * self.amplitude)
    }

    fn name(&self) -> &'static str { "waves" }
}

/// Voronoi cells - like cracked ground.
#[derive(Clone, Copy, Debug)]
pub struct Cells {
    pub frequency: f32,
    pub amplitude: f32,
    pub base: f32,
    pub seed: u32,
}

impl Default for Cells {
    fn default() -> Self {
        Self { frequency: 8.0, amplitude: 0.3, base: 0.5, seed: 42 }
    }
}

impl ZShape for Cells {
    fn depth(&self, x: usize, y: usize, width: usize, height: usize) -> Option<f32> {
        let nx = norm(x, width) * self.frequency;
        let ny = norm(y, height) * self.frequency;
        let n = noise::voronoi(nx, ny, self.seed);
        Some(self.base + (n - 0.5) * self.amplitude)
    }

    fn name(&self) -> &'static str { "cells" }
}

// ============================================================================
// Composite shapes
// ============================================================================

/// Multiple spheres at different depths.
#[derive(Clone, Debug)]
pub struct MultiSphere {
    pub spheres: Vec<Sphere>,
}

impl Default for MultiSphere {
    fn default() -> Self {
        Self {
            spheres: vec![
                Sphere { cx: 0.3, cy: 0.3, radius: 0.2, near: 0.2, far: 0.4 },
                Sphere { cx: 0.7, cy: 0.5, radius: 0.25, near: 0.4, far: 0.6 },
                Sphere { cx: 0.4, cy: 0.7, radius: 0.15, near: 0.6, far: 0.8 },
            ],
        }
    }
}

impl ZShape for MultiSphere {
    fn depth(&self, x: usize, y: usize, width: usize, height: usize) -> Option<f32> {
        let mut nearest: Option<f32> = None;
        for sphere in &self.spheres {
            if let Some(d) = sphere.depth(x, y, width, height) {
                nearest = Some(match nearest {
                    None => d,
                    Some(prev) => prev.min(d), // Nearest wins
                });
            }
        }
        nearest
    }

    fn name(&self) -> &'static str { "multi-sphere" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sphere_center() {
        let s = Sphere::default();
        let d = s.depth(50, 50, 100, 100);
        assert!(d.is_some());
        let d = d.unwrap();
        // Center should be near the "near" value
        assert!(d >= s.near && d <= s.far);
    }

    #[test]
    fn test_sphere_outside() {
        let s = Sphere::default();
        let d = s.depth(0, 0, 100, 100);
        assert!(d.is_none());
    }

    #[test]
    fn test_terrain_range() {
        let t = Terrain::default();
        for y in 0..10 {
            for x in 0..10 {
                let d = t.depth(x * 10, y * 10, 100, 100).unwrap();
                assert!(d >= 0.0 && d <= 1.0, "terrain depth out of range: {d}");
            }
        }
    }

    #[test]
    fn test_multi_sphere() {
        let ms = MultiSphere::default();
        // Should have some foreground pixels
        let mut found = false;
        for y in 0..100 {
            for x in 0..100 {
                if ms.depth(x, y, 100, 100).is_some() {
                    found = true;
                    break;
                }
            }
        }
        assert!(found);
    }
}
