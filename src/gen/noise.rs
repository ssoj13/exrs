//! Simple noise generators for procedural patterns.
//!
//! Implements basic Perlin-like noise without external dependencies.

use std::f32::consts::PI;

/// Simple hash function for deterministic pseudo-random values.
#[inline]
fn hash(x: i32, y: i32, seed: u32) -> u32 {
    let mut h = seed;
    h ^= x as u32;
    h = h.wrapping_mul(0x85ebca6b);
    h ^= y as u32;
    h = h.wrapping_mul(0xc2b2ae35);
    h ^= h >> 16;
    h
}

/// Hash to float in [0, 1] range.
#[inline]
fn hash_f(x: i32, y: i32, seed: u32) -> f32 {
    (hash(x, y, seed) & 0x7FFFFF) as f32 / 0x7FFFFF as f32
}

/// Smoothstep interpolation.
#[inline]
fn smoothstep(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// Linear interpolation.
#[inline]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + t * (b - a)
}

/// Value noise - smooth random values.
pub fn value_noise(x: f32, y: f32, seed: u32) -> f32 {
    let xi = x.floor() as i32;
    let yi = y.floor() as i32;
    let xf = x - x.floor();
    let yf = y - y.floor();

    // Smooth interpolation
    let u = smoothstep(xf);
    let v = smoothstep(yf);

    // Corner values
    let c00 = hash_f(xi, yi, seed);
    let c10 = hash_f(xi + 1, yi, seed);
    let c01 = hash_f(xi, yi + 1, seed);
    let c11 = hash_f(xi + 1, yi + 1, seed);

    // Bilinear interpolation
    let x0 = lerp(c00, c10, u);
    let x1 = lerp(c01, c11, u);
    lerp(x0, x1, v)
}

/// Gradient vectors for Perlin noise.
fn gradient(hash: u32) -> (f32, f32) {
    let angle = (hash & 0xFF) as f32 * (2.0 * PI / 256.0);
    (angle.cos(), angle.sin())
}

/// Perlin noise - smoother gradient-based noise.
pub fn perlin(x: f32, y: f32, seed: u32) -> f32 {
    let xi = x.floor() as i32;
    let yi = y.floor() as i32;
    let xf = x - x.floor();
    let yf = y - y.floor();

    // Smooth interpolation
    let u = smoothstep(xf);
    let v = smoothstep(yf);

    // Gradient dot products at corners
    let dot = |ix: i32, iy: i32, fx: f32, fy: f32| -> f32 {
        let (gx, gy) = gradient(hash(ix, iy, seed));
        gx * fx + gy * fy
    };

    let n00 = dot(xi, yi, xf, yf);
    let n10 = dot(xi + 1, yi, xf - 1.0, yf);
    let n01 = dot(xi, yi + 1, xf, yf - 1.0);
    let n11 = dot(xi + 1, yi + 1, xf - 1.0, yf - 1.0);

    // Bilinear interpolation
    let x0 = lerp(n00, n10, u);
    let x1 = lerp(n01, n11, u);
    let result = lerp(x0, x1, v);

    // Normalize to [0, 1]
    result * 0.5 + 0.5
}

/// Fractal Brownian Motion - layered noise.
pub fn fbm(x: f32, y: f32, octaves: u32, seed: u32) -> f32 {
    let mut value = 0.0;
    let mut amplitude = 0.5;
    let mut frequency = 1.0;
    let mut max_value = 0.0;

    for i in 0..octaves {
        value += amplitude * perlin(x * frequency, y * frequency, seed.wrapping_add(i));
        max_value += amplitude;
        amplitude *= 0.5;
        frequency *= 2.0;
    }

    value / max_value
}

/// Ridged noise - creates mountain-like ridges.
pub fn ridged(x: f32, y: f32, octaves: u32, seed: u32) -> f32 {
    let mut value = 0.0;
    let mut amplitude = 0.5;
    let mut frequency = 1.0;
    let mut max_value = 0.0;

    for i in 0..octaves {
        let n = perlin(x * frequency, y * frequency, seed.wrapping_add(i));
        // Invert and take absolute to create ridges
        let ridge = 1.0 - (n * 2.0 - 1.0).abs();
        value += amplitude * ridge * ridge;
        max_value += amplitude;
        amplitude *= 0.5;
        frequency *= 2.0;
    }

    value / max_value
}

/// Voronoi/Worley noise - cellular pattern.
pub fn voronoi(x: f32, y: f32, seed: u32) -> f32 {
    let xi = x.floor() as i32;
    let yi = y.floor() as i32;
    let xf = x - x.floor();
    let yf = y - y.floor();

    let mut min_dist = f32::MAX;

    // Check 3x3 neighborhood
    for dy in -1..=1 {
        for dx in -1..=1 {
            let nx = xi + dx;
            let ny = yi + dy;

            // Random point in cell
            let px = dx as f32 + hash_f(nx, ny, seed) - xf;
            let py = dy as f32 + hash_f(nx, ny, seed.wrapping_add(1)) - yf;

            let dist = (px * px + py * py).sqrt();
            min_dist = min_dist.min(dist);
        }
    }

    // Normalize (max distance in unit cell is sqrt(2))
    (min_dist / 1.414).min(1.0)
}

/// Simple sine waves.
pub fn waves(x: f32, y: f32, freq_x: f32, freq_y: f32, phase: f32) -> f32 {
    let wx = (x * freq_x * 2.0 * PI + phase).sin();
    let wy = (y * freq_y * 2.0 * PI).sin();
    (wx + wy) * 0.25 + 0.5
}

/// Concentric ripples from center.
pub fn ripples(x: f32, y: f32, cx: f32, cy: f32, freq: f32, phase: f32) -> f32 {
    let dx = x - cx;
    let dy = y - cy;
    let dist = (dx * dx + dy * dy).sqrt();
    (dist * freq * 2.0 * PI + phase).sin() * 0.5 + 0.5
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_value_noise_range() {
        for i in 0..100 {
            let x = i as f32 * 0.1;
            let y = i as f32 * 0.07;
            let v = value_noise(x, y, 42);
            assert!(v >= 0.0 && v <= 1.0, "value_noise out of range: {v}");
        }
    }

    #[test]
    fn test_perlin_range() {
        for i in 0..100 {
            let x = i as f32 * 0.1;
            let y = i as f32 * 0.07;
            let v = perlin(x, y, 42);
            assert!(v >= 0.0 && v <= 1.0, "perlin out of range: {v}");
        }
    }

    #[test]
    fn test_fbm_range() {
        for i in 0..50 {
            let x = i as f32 * 0.2;
            let y = i as f32 * 0.15;
            let v = fbm(x, y, 4, 42);
            assert!(v >= 0.0 && v <= 1.0, "fbm out of range: {v}");
        }
    }

    #[test]
    fn test_voronoi_range() {
        for i in 0..50 {
            let x = i as f32 * 0.2;
            let y = i as f32 * 0.15;
            let v = voronoi(x, y, 42);
            assert!(v >= 0.0 && v <= 1.0, "voronoi out of range: {v}");
        }
    }

    #[test]
    fn test_deterministic() {
        let v1 = perlin(1.5, 2.5, 42);
        let v2 = perlin(1.5, 2.5, 42);
        assert_eq!(v1, v2);
    }
}
