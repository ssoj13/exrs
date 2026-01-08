//! 2D pattern generators for image channels.

use super::noise;

/// Normalize pixel coord to [0, 1].
#[inline]
pub fn norm(coord: usize, size: usize) -> f32 {
    if size <= 1 { 0.5 } else { coord as f32 / (size - 1) as f32 }
}

/// Normalize to [-1, 1] centered.
#[inline]
pub fn norm_centered(coord: usize, size: usize) -> f32 {
    norm(coord, size) * 2.0 - 1.0
}

/// 2D pattern generator trait.
pub trait Pattern: Send + Sync {
    /// Generate value at (x, y) for image of given size.
    /// Returns value in [0, 1] range.
    fn sample(&self, x: usize, y: usize, width: usize, height: usize) -> f32;

    /// Pattern name for display.
    fn name(&self) -> &'static str;
}

// ============================================================================
// Gradient patterns
// ============================================================================

/// Horizontal gradient (left=0, right=1).
#[derive(Clone, Copy, Debug, Default)]
pub struct GradientH;

impl Pattern for GradientH {
    fn sample(&self, x: usize, _y: usize, width: usize, _height: usize) -> f32 {
        norm(x, width)
    }
    fn name(&self) -> &'static str { "gradient-h" }
}

/// Vertical gradient (top=0, bottom=1).
#[derive(Clone, Copy, Debug, Default)]
pub struct GradientV;

impl Pattern for GradientV {
    fn sample(&self, _x: usize, y: usize, _width: usize, height: usize) -> f32 {
        norm(y, height)
    }
    fn name(&self) -> &'static str { "gradient-v" }
}

/// Radial gradient (center=0, edges=1).
#[derive(Clone, Copy, Debug)]
pub struct GradientRadial {
    pub cx: f32,
    pub cy: f32,
}

impl Default for GradientRadial {
    fn default() -> Self { Self { cx: 0.5, cy: 0.5 } }
}

impl Pattern for GradientRadial {
    fn sample(&self, x: usize, y: usize, width: usize, height: usize) -> f32 {
        let nx = norm(x, width) - self.cx;
        let ny = norm(y, height) - self.cy;
        let dist = (nx * nx + ny * ny).sqrt();
        (dist * 2.0).min(1.0)
    }
    fn name(&self) -> &'static str { "gradient-radial" }
}

/// Angular gradient (0-360 degrees mapped to 0-1).
#[derive(Clone, Copy, Debug)]
pub struct GradientAngular {
    pub cx: f32,
    pub cy: f32,
}

impl Default for GradientAngular {
    fn default() -> Self { Self { cx: 0.5, cy: 0.5 } }
}

impl Pattern for GradientAngular {
    fn sample(&self, x: usize, y: usize, width: usize, height: usize) -> f32 {
        let nx = norm(x, width) - self.cx;
        let ny = norm(y, height) - self.cy;
        let angle = ny.atan2(nx);
        (angle / std::f32::consts::PI + 1.0) * 0.5
    }
    fn name(&self) -> &'static str { "gradient-angular" }
}

// ============================================================================
// Geometric patterns
// ============================================================================

/// Checkerboard pattern.
#[derive(Clone, Copy, Debug)]
pub struct Checker {
    pub cells_x: usize,
    pub cells_y: usize,
}

impl Default for Checker {
    fn default() -> Self { Self { cells_x: 8, cells_y: 8 } }
}

impl Pattern for Checker {
    fn sample(&self, x: usize, y: usize, width: usize, height: usize) -> f32 {
        let cx = (x * self.cells_x / width) % 2;
        let cy = (y * self.cells_y / height) % 2;
        if cx ^ cy == 0 { 0.0 } else { 1.0 }
    }
    fn name(&self) -> &'static str { "checker" }
}

/// Grid lines pattern.
#[derive(Clone, Copy, Debug)]
pub struct Grid {
    pub cells_x: usize,
    pub cells_y: usize,
    pub line_width: f32,
}

impl Default for Grid {
    fn default() -> Self { Self { cells_x: 8, cells_y: 8, line_width: 0.1 } }
}

impl Pattern for Grid {
    fn sample(&self, x: usize, y: usize, width: usize, height: usize) -> f32 {
        let cell_w = width as f32 / self.cells_x as f32;
        let cell_h = height as f32 / self.cells_y as f32;
        let fx = (x as f32 % cell_w) / cell_w;
        let fy = (y as f32 % cell_h) / cell_h;
        let half_line = self.line_width * 0.5;
        if fx < half_line || fx > 1.0 - half_line || fy < half_line || fy > 1.0 - half_line {
            1.0
        } else {
            0.0
        }
    }
    fn name(&self) -> &'static str { "grid" }
}

/// Dot/circle pattern.
#[derive(Clone, Copy, Debug)]
pub struct Dots {
    pub cells_x: usize,
    pub cells_y: usize,
    pub radius: f32,
}

impl Default for Dots {
    fn default() -> Self { Self { cells_x: 8, cells_y: 8, radius: 0.3 } }
}

impl Pattern for Dots {
    fn sample(&self, x: usize, y: usize, width: usize, height: usize) -> f32 {
        let cell_w = width as f32 / self.cells_x as f32;
        let cell_h = height as f32 / self.cells_y as f32;
        let fx = (x as f32 % cell_w) / cell_w - 0.5;
        let fy = (y as f32 % cell_h) / cell_h - 0.5;
        let dist = (fx * fx + fy * fy).sqrt();
        if dist < self.radius { 1.0 } else { 0.0 }
    }
    fn name(&self) -> &'static str { "dots" }
}

// ============================================================================
// Noise patterns
// ============================================================================

/// Perlin noise pattern.
#[derive(Clone, Copy, Debug)]
pub struct NoisePerlin {
    pub frequency: f32,
    pub seed: u32,
}

impl Default for NoisePerlin {
    fn default() -> Self { Self { frequency: 4.0, seed: 42 } }
}

impl Pattern for NoisePerlin {
    fn sample(&self, x: usize, y: usize, width: usize, height: usize) -> f32 {
        let nx = norm(x, width) * self.frequency;
        let ny = norm(y, height) * self.frequency;
        noise::perlin(nx, ny, self.seed)
    }
    fn name(&self) -> &'static str { "noise-perlin" }
}

/// FBM noise (layered Perlin).
#[derive(Clone, Copy, Debug)]
pub struct NoiseFbm {
    pub frequency: f32,
    pub octaves: u32,
    pub seed: u32,
}

impl Default for NoiseFbm {
    fn default() -> Self { Self { frequency: 4.0, octaves: 4, seed: 42 } }
}

impl Pattern for NoiseFbm {
    fn sample(&self, x: usize, y: usize, width: usize, height: usize) -> f32 {
        let nx = norm(x, width) * self.frequency;
        let ny = norm(y, height) * self.frequency;
        noise::fbm(nx, ny, self.octaves, self.seed)
    }
    fn name(&self) -> &'static str { "noise-fbm" }
}

/// Ridged noise (mountain-like).
#[derive(Clone, Copy, Debug)]
pub struct NoiseRidged {
    pub frequency: f32,
    pub octaves: u32,
    pub seed: u32,
}

impl Default for NoiseRidged {
    fn default() -> Self { Self { frequency: 4.0, octaves: 4, seed: 42 } }
}

impl Pattern for NoiseRidged {
    fn sample(&self, x: usize, y: usize, width: usize, height: usize) -> f32 {
        let nx = norm(x, width) * self.frequency;
        let ny = norm(y, height) * self.frequency;
        noise::ridged(nx, ny, self.octaves, self.seed)
    }
    fn name(&self) -> &'static str { "noise-ridged" }
}

/// Voronoi/cellular noise.
#[derive(Clone, Copy, Debug)]
pub struct NoiseVoronoi {
    pub frequency: f32,
    pub seed: u32,
}

impl Default for NoiseVoronoi {
    fn default() -> Self { Self { frequency: 8.0, seed: 42 } }
}

impl Pattern for NoiseVoronoi {
    fn sample(&self, x: usize, y: usize, width: usize, height: usize) -> f32 {
        let nx = norm(x, width) * self.frequency;
        let ny = norm(y, height) * self.frequency;
        noise::voronoi(nx, ny, self.seed)
    }
    fn name(&self) -> &'static str { "noise-voronoi" }
}

// ============================================================================
// Wave patterns
// ============================================================================

/// Sine waves pattern.
#[derive(Clone, Copy, Debug)]
pub struct Waves {
    pub freq_x: f32,
    pub freq_y: f32,
    pub phase: f32,
}

impl Default for Waves {
    fn default() -> Self { Self { freq_x: 4.0, freq_y: 4.0, phase: 0.0 } }
}

impl Pattern for Waves {
    fn sample(&self, x: usize, y: usize, width: usize, height: usize) -> f32 {
        let nx = norm(x, width);
        let ny = norm(y, height);
        noise::waves(nx, ny, self.freq_x, self.freq_y, self.phase)
    }
    fn name(&self) -> &'static str { "waves" }
}

/// Concentric ripples.
#[derive(Clone, Copy, Debug)]
pub struct Ripples {
    pub cx: f32,
    pub cy: f32,
    pub frequency: f32,
    pub phase: f32,
}

impl Default for Ripples {
    fn default() -> Self { Self { cx: 0.5, cy: 0.5, frequency: 8.0, phase: 0.0 } }
}

impl Pattern for Ripples {
    fn sample(&self, x: usize, y: usize, width: usize, height: usize) -> f32 {
        let nx = norm(x, width);
        let ny = norm(y, height);
        noise::ripples(nx, ny, self.cx, self.cy, self.frequency, self.phase)
    }
    fn name(&self) -> &'static str { "ripples" }
}

// ============================================================================
// Special patterns
// ============================================================================

/// UV map (R=U, G=V).
#[derive(Clone, Copy, Debug, Default)]
pub struct UvMapU;

impl Pattern for UvMapU {
    fn sample(&self, x: usize, _y: usize, width: usize, _height: usize) -> f32 {
        norm(x, width)
    }
    fn name(&self) -> &'static str { "uv-u" }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct UvMapV;

impl Pattern for UvMapV {
    fn sample(&self, _x: usize, y: usize, _width: usize, height: usize) -> f32 {
        1.0 - norm(y, height) // V is typically bottom-to-top
    }
    fn name(&self) -> &'static str { "uv-v" }
}

/// Solid color (constant value).
#[derive(Clone, Copy, Debug)]
pub struct Solid {
    pub value: f32,
}

impl Default for Solid {
    fn default() -> Self { Self { value: 0.5 } }
}

impl Pattern for Solid {
    fn sample(&self, _x: usize, _y: usize, _width: usize, _height: usize) -> f32 {
        self.value
    }
    fn name(&self) -> &'static str { "solid" }
}

/// Zone plate (Siemens star) - for aliasing tests.
#[derive(Clone, Copy, Debug)]
pub struct ZonePlate {
    pub frequency: f32,
}

impl Default for ZonePlate {
    fn default() -> Self { Self { frequency: 50.0 } }
}

impl Pattern for ZonePlate {
    fn sample(&self, x: usize, y: usize, width: usize, height: usize) -> f32 {
        let nx = norm_centered(x, width);
        let ny = norm_centered(y, height);
        let r2 = nx * nx + ny * ny;
        ((r2 * self.frequency * std::f32::consts::PI).sin() + 1.0) * 0.5
    }
    fn name(&self) -> &'static str { "zone-plate" }
}

// ============================================================================
// Color bars (SMPTE-like)
// ============================================================================

/// SMPTE color bars - returns bar index 0-7.
#[derive(Clone, Copy, Debug, Default)]
pub struct ColorBars;

impl ColorBars {
    /// Get RGB for bar index.
    pub fn bar_color(index: usize) -> (f32, f32, f32) {
        match index % 8 {
            0 => (0.75, 0.75, 0.75), // white (75%)
            1 => (0.75, 0.75, 0.0),  // yellow
            2 => (0.0, 0.75, 0.75),  // cyan
            3 => (0.0, 0.75, 0.0),   // green
            4 => (0.75, 0.0, 0.75),  // magenta
            5 => (0.75, 0.0, 0.0),   // red
            6 => (0.0, 0.0, 0.75),   // blue
            _ => (0.0, 0.0, 0.0),    // black
        }
    }
}

impl Pattern for ColorBars {
    fn sample(&self, x: usize, _y: usize, width: usize, _height: usize) -> f32 {
        let bar = x * 8 / width;
        bar as f32 / 7.0
    }
    fn name(&self) -> &'static str { "color-bars" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gradient_h() {
        let p = GradientH;
        assert!((p.sample(0, 0, 100, 100) - 0.0).abs() < 0.01);
        assert!((p.sample(99, 0, 100, 100) - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_checker() {
        let p = Checker { cells_x: 2, cells_y: 2 };
        assert_eq!(p.sample(0, 0, 100, 100), 0.0);
        assert_eq!(p.sample(75, 0, 100, 100), 1.0);
        assert_eq!(p.sample(75, 75, 100, 100), 0.0);
    }

    #[test]
    fn test_all_patterns_in_range() {
        let patterns: Vec<Box<dyn Pattern>> = vec![
            Box::new(GradientH),
            Box::new(GradientV),
            Box::new(GradientRadial::default()),
            Box::new(Checker::default()),
            Box::new(NoisePerlin::default()),
            Box::new(NoiseFbm::default()),
            Box::new(Waves::default()),
        ];

        for p in &patterns {
            for y in 0..10 {
                for x in 0..10 {
                    let v = p.sample(x * 10, y * 10, 100, 100);
                    assert!(v >= 0.0 && v <= 1.0, "{} out of range: {v}", p.name());
                }
            }
        }
    }
}
