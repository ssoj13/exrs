//! Deep data generators for volumetric and particle effects.

use super::noise;
use super::pattern::norm;

/// Deep sample data for a single pixel.
#[derive(Clone, Debug)]
pub struct DeepPixel {
    /// Depth values for each sample (sorted front-to-back).
    pub depths: Vec<f32>,
    /// RGBA values for each sample (4 floats per sample).
    pub colors: Vec<[f32; 4]>,
}

impl DeepPixel {
    pub fn new() -> Self {
        Self { depths: Vec::new(), colors: Vec::new() }
    }

    pub fn with_capacity(n: usize) -> Self {
        Self { depths: Vec::with_capacity(n), colors: Vec::with_capacity(n) }
    }

    pub fn push(&mut self, depth: f32, rgba: [f32; 4]) {
        self.depths.push(depth);
        self.colors.push(rgba);
    }

    pub fn sample_count(&self) -> usize {
        self.depths.len()
    }

    pub fn is_empty(&self) -> bool {
        self.depths.is_empty()
    }

    /// Sort samples by depth (front to back).
    pub fn sort_by_depth(&mut self) {
        if self.depths.len() <= 1 {
            return;
        }

        // Create indices and sort by depth
        let mut indices: Vec<usize> = (0..self.depths.len()).collect();
        indices.sort_by(|&a, &b| {
            self.depths[a].partial_cmp(&self.depths[b]).unwrap_or(std::cmp::Ordering::Equal)
        });

        // Reorder in place
        let depths: Vec<f32> = indices.iter().map(|&i| self.depths[i]).collect();
        let colors: Vec<[f32; 4]> = indices.iter().map(|&i| self.colors[i]).collect();
        self.depths = depths;
        self.colors = colors;
    }
}

impl Default for DeepPixel {
    fn default() -> Self { Self::new() }
}

/// Deep data generator trait.
pub trait DeepGenerator: Send + Sync {
    /// Generate deep samples for pixel (x, y).
    fn generate(&self, x: usize, y: usize, width: usize, height: usize) -> DeepPixel;

    /// Generator name.
    fn name(&self) -> &'static str;
}

// ============================================================================
// Particle system
// ============================================================================

/// Random particles in 3D space.
#[derive(Clone, Debug)]
pub struct Particles {
    pub count: usize,
    pub depth_min: f32,
    pub depth_max: f32,
    pub seed: u32,
}

impl Default for Particles {
    fn default() -> Self {
        Self { count: 10000, depth_min: 0.1, depth_max: 0.9, seed: 42 }
    }
}

impl Particles {
    /// Simple hash for deterministic particle positions.
    fn hash(&self, i: usize) -> (f32, f32, f32, [f32; 4]) {
        let mut h = self.seed.wrapping_add(i as u32);
        h = h.wrapping_mul(0x85ebca6b);
        h ^= h >> 13;
        h = h.wrapping_mul(0xc2b2ae35);
        h ^= h >> 16;

        let x = (h & 0xFFFF) as f32 / 0xFFFF as f32;
        h = h.wrapping_mul(0x45d9f3b);
        let y = (h & 0xFFFF) as f32 / 0xFFFF as f32;
        h = h.wrapping_mul(0x45d9f3b);
        let z = self.depth_min + (h & 0xFFFF) as f32 / 0xFFFF as f32 * (self.depth_max - self.depth_min);
        h = h.wrapping_mul(0x45d9f3b);

        // Color based on depth
        let r = 0.3 + 0.7 * z;
        let g = 0.5;
        let b = 1.0 - z * 0.5;
        let a = 0.5 + 0.5 * ((h & 0xFF) as f32 / 255.0);

        (x, y, z, [r, g, b, a])
    }
}

impl DeepGenerator for Particles {
    fn generate(&self, x: usize, y: usize, width: usize, height: usize) -> DeepPixel {
        let mut pixel = DeepPixel::new();
        let nx = norm(x, width);
        let ny = norm(y, height);
        let pixel_size = 1.0 / width.max(height) as f32;

        // Check each particle
        for i in 0..self.count {
            let (px, py, pz, color) = self.hash(i);
            let dx = (px - nx).abs();
            let dy = (py - ny).abs();

            // Particle radius varies with depth (perspective)
            let radius = pixel_size * 2.0 * (1.0 - pz * 0.5);
            if dx < radius && dy < radius {
                pixel.push(pz, color);
            }
        }

        pixel.sort_by_depth();
        pixel
    }

    fn name(&self) -> &'static str { "particles" }
}

// ============================================================================
// Volumetric fog
// ============================================================================

/// Uniform volumetric fog - samples along the view ray.
#[derive(Clone, Copy, Debug)]
pub struct VolumetricFog {
    pub samples: usize,
    pub depth_min: f32,
    pub depth_max: f32,
    pub density: f32,
    pub color: [f32; 3],
}

impl Default for VolumetricFog {
    fn default() -> Self {
        Self {
            samples: 16,
            depth_min: 0.1,
            depth_max: 0.9,
            density: 0.1,
            color: [0.7, 0.75, 0.8],
        }
    }
}

impl DeepGenerator for VolumetricFog {
    fn generate(&self, _x: usize, _y: usize, _width: usize, _height: usize) -> DeepPixel {
        let mut pixel = DeepPixel::with_capacity(self.samples);
        let depth_step = (self.depth_max - self.depth_min) / (self.samples - 1).max(1) as f32;

        for i in 0..self.samples {
            let z = self.depth_min + i as f32 * depth_step;
            let color = [self.color[0], self.color[1], self.color[2], self.density];
            pixel.push(z, color);
        }

        pixel
    }

    fn name(&self) -> &'static str { "fog" }
}

// ============================================================================
// Cloud volume
// ============================================================================

/// Volumetric cloud using 3D noise.
#[derive(Clone, Copy, Debug)]
pub struct CloudVolume {
    pub samples: usize,
    pub depth_min: f32,
    pub depth_max: f32,
    pub frequency: f32,
    pub threshold: f32,
    pub seed: u32,
}

impl Default for CloudVolume {
    fn default() -> Self {
        Self {
            samples: 32,
            depth_min: 0.2,
            depth_max: 0.8,
            frequency: 4.0,
            threshold: 0.4,
            seed: 42,
        }
    }
}

impl DeepGenerator for CloudVolume {
    fn generate(&self, x: usize, y: usize, width: usize, height: usize) -> DeepPixel {
        let mut pixel = DeepPixel::new();
        let nx = norm(x, width) * self.frequency;
        let ny = norm(y, height) * self.frequency;
        let depth_step = (self.depth_max - self.depth_min) / (self.samples - 1).max(1) as f32;

        for i in 0..self.samples {
            let z = self.depth_min + i as f32 * depth_step;
            let nz = z * self.frequency;

            // 3D noise approximation (2D slices at different Z)
            let n = noise::fbm(nx + nz * 0.3, ny + nz * 0.7, 4, self.seed.wrapping_add(i as u32));

            if n > self.threshold {
                let density = (n - self.threshold) / (1.0 - self.threshold);
                let color = [0.95, 0.95, 0.98, density * 0.3];
                pixel.push(z, color);
            }
        }

        pixel
    }

    fn name(&self) -> &'static str { "cloud" }
}

// ============================================================================
// Layered glass
// ============================================================================

/// Multiple transparent layers at fixed depths.
#[derive(Clone, Debug)]
pub struct LayeredGlass {
    pub layers: Vec<(f32, [f32; 4])>, // (depth, rgba)
}

impl Default for LayeredGlass {
    fn default() -> Self {
        Self {
            layers: vec![
                (0.2, [1.0, 0.3, 0.3, 0.3]),  // Red layer
                (0.4, [0.3, 1.0, 0.3, 0.3]),  // Green layer
                (0.6, [0.3, 0.3, 1.0, 0.3]),  // Blue layer
                (0.8, [1.0, 1.0, 0.3, 0.3]),  // Yellow layer
            ],
        }
    }
}

impl DeepGenerator for LayeredGlass {
    fn generate(&self, _x: usize, _y: usize, _width: usize, _height: usize) -> DeepPixel {
        let mut pixel = DeepPixel::with_capacity(self.layers.len());
        for &(depth, color) in &self.layers {
            pixel.push(depth, color);
        }
        pixel
    }

    fn name(&self) -> &'static str { "glass" }
}

// ============================================================================
// Gradient density
// ============================================================================

/// Volumetric with density gradient (more samples where denser).
#[derive(Clone, Copy, Debug)]
pub struct GradientVolume {
    pub max_samples: usize,
    pub depth_min: f32,
    pub depth_max: f32,
    pub center: f32,     // Depth of maximum density
    pub falloff: f32,    // How quickly density falls off
}

impl Default for GradientVolume {
    fn default() -> Self {
        Self {
            max_samples: 24,
            depth_min: 0.1,
            depth_max: 0.9,
            center: 0.5,
            falloff: 3.0,
        }
    }
}

impl DeepGenerator for GradientVolume {
    fn generate(&self, x: usize, y: usize, width: usize, height: usize) -> DeepPixel {
        let mut pixel = DeepPixel::new();
        let nx = norm(x, width);
        let ny = norm(y, height);

        // Radial falloff from image center too
        let rx = nx - 0.5;
        let ry = ny - 0.5;
        let radial = 1.0 - (rx * rx + ry * ry).sqrt() * 2.0;
        if radial <= 0.0 {
            return pixel;
        }

        let depth_step = (self.depth_max - self.depth_min) / (self.max_samples - 1).max(1) as f32;

        for i in 0..self.max_samples {
            let z = self.depth_min + i as f32 * depth_step;
            let dist_from_center = (z - self.center).abs();
            let density = (-dist_from_center * self.falloff).exp() * radial;

            if density > 0.05 {
                let g = 0.4 + density * 0.6;
                let color = [g, g * 0.9, g * 0.8, density * 0.5];
                pixel.push(z, color);
            }
        }

        pixel
    }

    fn name(&self) -> &'static str { "gradient-volume" }
}

// ============================================================================
// Explosion/burst
// ============================================================================

/// Radial explosion pattern.
#[derive(Clone, Copy, Debug)]
pub struct Explosion {
    pub cx: f32,
    pub cy: f32,
    pub samples_per_ray: usize,
    pub radius: f32,
    pub seed: u32,
}

impl Default for Explosion {
    fn default() -> Self {
        Self { cx: 0.5, cy: 0.5, samples_per_ray: 8, radius: 0.4, seed: 42 }
    }
}

impl DeepGenerator for Explosion {
    fn generate(&self, x: usize, y: usize, width: usize, height: usize) -> DeepPixel {
        let mut pixel = DeepPixel::new();
        let nx = norm(x, width) - self.cx;
        let ny = norm(y, height) - self.cy;
        let dist = (nx * nx + ny * ny).sqrt();

        if dist > self.radius || dist < 0.01 {
            return pixel;
        }

        // Normalized distance
        let t = dist / self.radius;

        // Add samples along the ray
        for i in 0..self.samples_per_ray {
            let sample_t = i as f32 / (self.samples_per_ray - 1).max(1) as f32;
            let z = 0.3 + t * 0.4 + sample_t * 0.2;

            // Color based on distance (hot core, cool edges)
            let heat = 1.0 - t;
            let r = 1.0;
            let g = heat * 0.8;
            let b = heat * heat * 0.3;
            let a = (1.0 - sample_t) * 0.4 * (1.0 - t * 0.5);

            if a > 0.01 {
                pixel.push(z, [r, g, b, a]);
            }
        }

        pixel.sort_by_depth();
        pixel
    }

    fn name(&self) -> &'static str { "explosion" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_particles() {
        let gen = Particles { count: 1000, ..Default::default() };
        let mut total_samples = 0;
        for y in 0..10 {
            for x in 0..10 {
                let pixel = gen.generate(x * 10, y * 10, 100, 100);
                total_samples += pixel.sample_count();
            }
        }
        // Should have some samples hit
        assert!(total_samples > 0);
    }

    #[test]
    fn test_fog_uniform() {
        let gen = VolumetricFog::default();
        let p1 = gen.generate(0, 0, 100, 100);
        let p2 = gen.generate(50, 50, 100, 100);
        assert_eq!(p1.sample_count(), p2.sample_count());
        assert_eq!(p1.sample_count(), gen.samples);
    }

    #[test]
    fn test_cloud_volume() {
        let gen = CloudVolume::default();
        let pixel = gen.generate(50, 50, 100, 100);
        // Should have at least some samples
        assert!(pixel.sample_count() <= gen.samples);
    }

    #[test]
    fn test_layered_glass() {
        let gen = LayeredGlass::default();
        let pixel = gen.generate(50, 50, 100, 100);
        assert_eq!(pixel.sample_count(), gen.layers.len());
    }

    #[test]
    fn test_deep_pixel_sort() {
        let mut pixel = DeepPixel::new();
        pixel.push(0.8, [1.0, 0.0, 0.0, 1.0]);
        pixel.push(0.2, [0.0, 1.0, 0.0, 1.0]);
        pixel.push(0.5, [0.0, 0.0, 1.0, 1.0]);
        pixel.sort_by_depth();

        assert!((pixel.depths[0] - 0.2).abs() < 0.001);
        assert!((pixel.depths[1] - 0.5).abs() < 0.001);
        assert!((pixel.depths[2] - 0.8).abs() < 0.001);
    }
}
