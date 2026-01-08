//! Test image generator module for EXR files.
//!
// Allow missing Copy for types with Vec fields
#![allow(missing_copy_implementations)]
#![allow(missing_docs)] // Generator module has inline comments
//!
//! Generates test images with various patterns, shapes, and deep data
//! for testing and development purposes.
//!
//! # Example
//!
//! ```ignore
//! use exr::gen::{TestImage, PatternType, ChannelSpec};
//!
//! let img = TestImage::builder()
//!     .size(1920, 1080)
//!     .pattern(PatternType::Gradient)
//!     .channels(ChannelSpec::Rgba)
//!     .build()?;
//!
//! img.write("test.exr")?;
//! ```

pub mod noise;
pub mod pattern;
pub mod shape;
pub mod deep;

use std::path::Path;
use crate::prelude::*;
use crate::image::write::WritableImage;
use crate::image::deep::{DeepSamples, DeepChannelData};
use crate::image::write::deep::write_deep_scanlines_to_file;
use crate::meta::attribute::{ChannelList, ChannelDescription, SampleType};

pub use pattern::Pattern;
pub use shape::ZShape;
pub use deep::{DeepGenerator, DeepPixel};

// ============================================================================
// Pattern types enum
// ============================================================================

/// Available 2D patterns.
#[derive(Clone, Copy, Debug)]
pub enum PatternType {
    GradientH,
    GradientV,
    GradientRadial,
    GradientAngular,
    Checker { cells: usize },
    Grid { cells: usize },
    Dots { cells: usize },
    NoisePerlin { freq: f32, seed: u32 },
    NoiseFbm { freq: f32, octaves: u32, seed: u32 },
    NoiseRidged { freq: f32, octaves: u32, seed: u32 },
    NoiseVoronoi { freq: f32, seed: u32 },
    Waves { freq: f32 },
    Ripples { freq: f32 },
    ZonePlate { freq: f32 },
    ColorBars,
    UvMap,
    Solid { value: f32 },
}

impl Default for PatternType {
    fn default() -> Self { Self::GradientH }
}

impl PatternType {
    /// Sample the pattern at (x, y).
    pub fn sample(&self, x: usize, y: usize, w: usize, h: usize) -> f32 {
        match self {
            Self::GradientH => pattern::GradientH.sample(x, y, w, h),
            Self::GradientV => pattern::GradientV.sample(x, y, w, h),
            Self::GradientRadial => pattern::GradientRadial::default().sample(x, y, w, h),
            Self::GradientAngular => pattern::GradientAngular::default().sample(x, y, w, h),
            Self::Checker { cells } => pattern::Checker { cells_x: *cells, cells_y: *cells }.sample(x, y, w, h),
            Self::Grid { cells } => pattern::Grid { cells_x: *cells, cells_y: *cells, line_width: 0.1 }.sample(x, y, w, h),
            Self::Dots { cells } => pattern::Dots { cells_x: *cells, cells_y: *cells, radius: 0.3 }.sample(x, y, w, h),
            Self::NoisePerlin { freq, seed } => pattern::NoisePerlin { frequency: *freq, seed: *seed }.sample(x, y, w, h),
            Self::NoiseFbm { freq, octaves, seed } => pattern::NoiseFbm { frequency: *freq, octaves: *octaves, seed: *seed }.sample(x, y, w, h),
            Self::NoiseRidged { freq, octaves, seed } => pattern::NoiseRidged { frequency: *freq, octaves: *octaves, seed: *seed }.sample(x, y, w, h),
            Self::NoiseVoronoi { freq, seed } => pattern::NoiseVoronoi { frequency: *freq, seed: *seed }.sample(x, y, w, h),
            Self::Waves { freq } => pattern::Waves { freq_x: *freq, freq_y: *freq, phase: 0.0 }.sample(x, y, w, h),
            Self::Ripples { freq } => pattern::Ripples { frequency: *freq, ..Default::default() }.sample(x, y, w, h),
            Self::ZonePlate { freq } => pattern::ZonePlate { frequency: *freq }.sample(x, y, w, h),
            Self::ColorBars => pattern::ColorBars.sample(x, y, w, h),
            Self::UvMap => pattern::UvMapU.sample(x, y, w, h), // Just U component
            Self::Solid { value } => *value,
        }
    }

    /// Parse from string.
    pub fn parse(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.split(':').collect();
        let name = parts[0].to_lowercase();
        let params: Vec<f32> = parts[1..].iter().filter_map(|p| p.parse().ok()).collect();

        match name.as_str() {
            "gradient-h" | "gradh" | "gh" => Some(Self::GradientH),
            "gradient-v" | "gradv" | "gv" => Some(Self::GradientV),
            "gradient-radial" | "gradr" | "gr" => Some(Self::GradientRadial),
            "gradient-angular" | "grada" | "ga" => Some(Self::GradientAngular),
            "checker" | "check" | "ch" => Some(Self::Checker { cells: params.first().map(|&f| f as usize).unwrap_or(8) }),
            "grid" => Some(Self::Grid { cells: params.first().map(|&f| f as usize).unwrap_or(8) }),
            "dots" => Some(Self::Dots { cells: params.first().map(|&f| f as usize).unwrap_or(8) }),
            "noise" | "perlin" | "np" => Some(Self::NoisePerlin {
                freq: params.first().copied().unwrap_or(4.0),
                seed: params.get(1).map(|&f| f as u32).unwrap_or(42),
            }),
            "fbm" | "nf" => Some(Self::NoiseFbm {
                freq: params.first().copied().unwrap_or(4.0),
                octaves: params.get(1).map(|&f| f as u32).unwrap_or(4),
                seed: params.get(2).map(|&f| f as u32).unwrap_or(42),
            }),
            "ridged" | "nr" => Some(Self::NoiseRidged {
                freq: params.first().copied().unwrap_or(4.0),
                octaves: params.get(1).map(|&f| f as u32).unwrap_or(4),
                seed: params.get(2).map(|&f| f as u32).unwrap_or(42),
            }),
            "voronoi" | "nv" | "cells" => Some(Self::NoiseVoronoi {
                freq: params.first().copied().unwrap_or(8.0),
                seed: params.get(1).map(|&f| f as u32).unwrap_or(42),
            }),
            "waves" | "wave" | "w" => Some(Self::Waves { freq: params.first().copied().unwrap_or(4.0) }),
            "ripples" | "ripple" | "rp" => Some(Self::Ripples { freq: params.first().copied().unwrap_or(8.0) }),
            "zoneplate" | "zone" | "zp" => Some(Self::ZonePlate { freq: params.first().copied().unwrap_or(50.0) }),
            "colorbars" | "bars" | "cb" => Some(Self::ColorBars),
            "uv" | "uvmap" => Some(Self::UvMap),
            "solid" | "s" => Some(Self::Solid { value: params.first().copied().unwrap_or(0.5) }),
            _ => None,
        }
    }

    /// List all available patterns.
    pub fn list() -> &'static [&'static str] {
        &[
            "gradient-h (gh)", "gradient-v (gv)", "gradient-radial (gr)", "gradient-angular (ga)",
            "checker:N (ch)", "grid:N", "dots:N",
            "noise:freq:seed (np)", "fbm:freq:oct:seed (nf)", "ridged:freq:oct:seed (nr)", "voronoi:freq:seed (nv)",
            "waves:freq (w)", "ripples:freq (rp)", "zoneplate:freq (zp)",
            "colorbars (cb)", "uv", "solid:val (s)"
        ]
    }
}

// ============================================================================
// Shape types enum
// ============================================================================

/// Available Z-depth shapes.
#[derive(Clone, Debug)]
pub enum ShapeType {
    Sphere,
    Box,
    Plane { angle: f32 },
    Cone,
    Cylinder,
    Torus,
    Terrain { freq: f32, seed: u32 },
    Mountains { freq: f32, seed: u32 },
    WaveSurface { freq: f32 },
    Cells { freq: f32, seed: u32 },
    MultiSphere,
}

impl Default for ShapeType {
    fn default() -> Self { Self::Sphere }
}

impl ShapeType {
    /// Sample depth at (x, y). Returns value in [0, 1].
    pub fn sample(&self, x: usize, y: usize, w: usize, h: usize) -> f32 {
        match self {
            Self::Sphere => shape::Sphere::default().sample(x, y, w, h, 1.0),
            Self::Box => shape::Box::default().sample(x, y, w, h, 1.0),
            Self::Plane { angle } => shape::Plane { angle: *angle, ..Default::default() }.sample(x, y, w, h, 1.0),
            Self::Cone => shape::Cone::default().sample(x, y, w, h, 1.0),
            Self::Cylinder => shape::Cylinder::default().sample(x, y, w, h, 1.0),
            Self::Torus => shape::Torus::default().sample(x, y, w, h, 1.0),
            Self::Terrain { freq, seed } => shape::Terrain { frequency: *freq, seed: *seed, ..Default::default() }.sample(x, y, w, h, 1.0),
            Self::Mountains { freq, seed } => shape::Mountains { frequency: *freq, seed: *seed, ..Default::default() }.sample(x, y, w, h, 1.0),
            Self::WaveSurface { freq } => shape::WaveSurface { freq_x: *freq, freq_y: *freq, ..Default::default() }.sample(x, y, w, h, 1.0),
            Self::Cells { freq, seed } => shape::Cells { frequency: *freq, seed: *seed, ..Default::default() }.sample(x, y, w, h, 1.0),
            Self::MultiSphere => shape::MultiSphere::default().sample(x, y, w, h, 1.0),
        }
    }

    /// Parse from string.
    pub fn parse(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.split(':').collect();
        let name = parts[0].to_lowercase();
        let params: Vec<f32> = parts[1..].iter().filter_map(|p| p.parse().ok()).collect();

        match name.as_str() {
            "sphere" | "sp" => Some(Self::Sphere),
            "box" | "cube" | "bx" => Some(Self::Box),
            "plane" | "pl" => Some(Self::Plane { angle: params.first().copied().unwrap_or(0.0) }),
            "cone" | "cn" => Some(Self::Cone),
            "cylinder" | "cyl" => Some(Self::Cylinder),
            "torus" | "tor" => Some(Self::Torus),
            "terrain" | "ter" => Some(Self::Terrain {
                freq: params.first().copied().unwrap_or(4.0),
                seed: params.get(1).map(|&f| f as u32).unwrap_or(42),
            }),
            "mountains" | "mtn" => Some(Self::Mountains {
                freq: params.first().copied().unwrap_or(3.0),
                seed: params.get(1).map(|&f| f as u32).unwrap_or(42),
            }),
            "wavesurface" | "ws" => Some(Self::WaveSurface { freq: params.first().copied().unwrap_or(4.0) }),
            "cells" | "voronoi" => Some(Self::Cells {
                freq: params.first().copied().unwrap_or(8.0),
                seed: params.get(1).map(|&f| f as u32).unwrap_or(42),
            }),
            "multisphere" | "multi" | "ms" => Some(Self::MultiSphere),
            _ => None,
        }
    }

    /// List available shapes.
    pub fn list() -> &'static [&'static str] {
        &[
            "sphere (sp)", "box (bx)", "plane:angle (pl)", "cone (cn)", "cylinder (cyl)", "torus (tor)",
            "terrain:freq:seed (ter)", "mountains:freq:seed (mtn)", "wavesurface:freq (ws)",
            "cells:freq:seed", "multisphere (ms)"
        ]
    }
}

// ============================================================================
// Deep types enum
// ============================================================================

/// Available deep data types.
#[derive(Clone, Debug)]
pub enum DeepType {
    Particles { count: usize, seed: u32 },
    Fog { samples: usize },
    Cloud { samples: usize, seed: u32 },
    Glass,
    GradientVolume { samples: usize },
    Explosion { seed: u32 },
}

impl Default for DeepType {
    fn default() -> Self { Self::Particles { count: 10000, seed: 42 } }
}

impl DeepType {
    /// Generate deep pixel at (x, y).
    pub fn generate(&self, x: usize, y: usize, w: usize, h: usize) -> DeepPixel {
        match self {
            Self::Particles { count, seed } => {
                deep::Particles { count: *count, seed: *seed, ..Default::default() }.generate(x, y, w, h)
            }
            Self::Fog { samples } => {
                deep::VolumetricFog { samples: *samples, ..Default::default() }.generate(x, y, w, h)
            }
            Self::Cloud { samples, seed } => {
                deep::CloudVolume { samples: *samples, seed: *seed, ..Default::default() }.generate(x, y, w, h)
            }
            Self::Glass => deep::LayeredGlass::default().generate(x, y, w, h),
            Self::GradientVolume { samples } => {
                deep::GradientVolume { max_samples: *samples, ..Default::default() }.generate(x, y, w, h)
            }
            Self::Explosion { seed } => {
                deep::Explosion { seed: *seed, ..Default::default() }.generate(x, y, w, h)
            }
        }
    }

    /// Parse from string.
    pub fn parse(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.split(':').collect();
        let name = parts[0].to_lowercase();
        let params: Vec<usize> = parts[1..].iter().filter_map(|p| p.parse().ok()).collect();

        match name.as_str() {
            "particles" | "part" | "p" => Some(Self::Particles {
                count: params.first().copied().unwrap_or(10000),
                seed: params.get(1).copied().unwrap_or(42) as u32,
            }),
            "fog" | "f" => Some(Self::Fog { samples: params.first().copied().unwrap_or(16) }),
            "cloud" | "cl" => Some(Self::Cloud {
                samples: params.first().copied().unwrap_or(32),
                seed: params.get(1).copied().unwrap_or(42) as u32,
            }),
            "glass" | "gl" => Some(Self::Glass),
            "gradient" | "grad" | "gv" => Some(Self::GradientVolume { samples: params.first().copied().unwrap_or(24) }),
            "explosion" | "exp" | "e" => Some(Self::Explosion { seed: params.first().copied().unwrap_or(42) as u32 }),
            _ => None,
        }
    }

    /// List available deep types.
    pub fn list() -> &'static [&'static str] {
        &[
            "particles:count:seed (p)", "fog:samples (f)", "cloud:samples:seed (cl)",
            "glass (gl)", "gradient:samples (gv)", "explosion:seed (e)"
        ]
    }
}

// ============================================================================
// Channel specification
// ============================================================================

/// Channel configuration.
#[derive(Clone, Debug)]
pub enum ChannelSpec {
    Rgb,
    Rgba,
    RgbZ,
    RgbaZ,
    Z,
    Custom(Vec<String>),
}

impl Default for ChannelSpec {
    fn default() -> Self { Self::Rgba }
}

impl ChannelSpec {
    /// Parse from string.
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "rgb" => Some(Self::Rgb),
            "rgba" => Some(Self::Rgba),
            "rgbz" => Some(Self::RgbZ),
            "rgbaz" => Some(Self::RgbaZ),
            "z" | "depth" => Some(Self::Z),
            _ => {
                // Custom: comma-separated channel names
                let channels: Vec<String> = s.split(',').map(|c| c.trim().to_string()).collect();
                if channels.is_empty() { None } else { Some(Self::Custom(channels)) }
            }
        }
    }

    /// List available specs.
    pub fn list() -> &'static [&'static str] {
        &["rgb", "rgba", "rgbz", "rgbaz", "z", "R,G,B,custom,..."]
    }
}

// ============================================================================
// Image generation
// ============================================================================

/// Generate and write a flat (non-deep) test image.
pub fn generate_flat<P: AsRef<Path>>(
    path: P,
    width: usize,
    height: usize,
    pattern: &PatternType,
    z_shape: Option<&ShapeType>,
    channels: &ChannelSpec,
) -> crate::error::UnitResult {
    // Determine what channels to write
    let has_z = matches!(channels, ChannelSpec::RgbZ | ChannelSpec::RgbaZ | ChannelSpec::Z);
    let has_alpha = matches!(channels, ChannelSpec::Rgba | ChannelSpec::RgbaZ);
    let has_rgb = !matches!(channels, ChannelSpec::Z);

    if has_rgb {
        // RGB(A)(Z) image
        let z_shape = z_shape.cloned().unwrap_or_default();

        if has_z {
            if has_alpha {
                // RGBAZ
                let image = Image::from_channels(
                    (width, height),
                    SpecificChannels::build()
                        .with_channel("R")
                        .with_channel("G")
                        .with_channel("B")
                        .with_channel("A")
                        .with_channel("Z")
                        .with_pixels(move |pos: Vec2<usize>| {
                            let v = pattern.sample(pos.x(), pos.y(), width, height);
                            let z = z_shape.sample(pos.x(), pos.y(), width, height);
                            (v, v * 0.8, v * 0.6, 1.0f32, z)
                        })
                );
                image.write().to_file(path)?;
            } else {
                // RGBZ
                let image = Image::from_channels(
                    (width, height),
                    SpecificChannels::build()
                        .with_channel("R")
                        .with_channel("G")
                        .with_channel("B")
                        .with_channel("Z")
                        .with_pixels(move |pos: Vec2<usize>| {
                            let v = pattern.sample(pos.x(), pos.y(), width, height);
                            let z = z_shape.sample(pos.x(), pos.y(), width, height);
                            (v, v * 0.8, v * 0.6, z)
                        })
                );
                image.write().to_file(path)?;
            }
        } else if has_alpha {
            // RGBA
            let image = Image::from_channels(
                (width, height),
                SpecificChannels::rgba(move |pos: Vec2<usize>| {
                    let v = pattern.sample(pos.x(), pos.y(), width, height);
                    (v, v * 0.8, v * 0.6, 1.0f32)
                })
            );
            image.write().to_file(path)?;
        } else {
            // RGB
            let image = Image::from_channels(
                (width, height),
                SpecificChannels::rgb(move |pos: Vec2<usize>| {
                    let v = pattern.sample(pos.x(), pos.y(), width, height);
                    (v, v * 0.8, v * 0.6)
                })
            );
            image.write().to_file(path)?;
        }
    } else {
        // Z only
        let z_shape = z_shape.cloned().unwrap_or_default();
        let image = Image::from_channels(
            (width, height),
            SpecificChannels::build()
                .with_channel("Z")
                .with_pixels(move |pos: Vec2<usize>| {
                    (z_shape.sample(pos.x(), pos.y(), width, height),)
                })
        );
        image.write().to_file(path)?;
    }

    Ok(())
}

/// Generate and write a deep EXR test image.
pub fn generate_deep<P: AsRef<Path>>(
    path: P,
    width: usize,
    height: usize,
    deep_type: &DeepType,
) -> crate::error::UnitResult {
    println!("Generating deep image {}x{} with {} ...", width, height, deep_type_name(deep_type));
    
    let pixel_count = width * height;
    
    // First pass: generate all pixels to compute sample counts
    let mut pixels: Vec<DeepPixel> = Vec::with_capacity(pixel_count);
    let mut total_samples = 0usize;
    
    for y in 0..height {
        for x in 0..width {
            let pixel = deep_type.generate(x, y, width, height);
            total_samples += pixel.sample_count();
            pixels.push(pixel);
        }
    }
    
    println!("  Total samples: {} ({:.1} avg/pixel)", 
             total_samples, total_samples as f64 / pixel_count as f64);
    
    // Build cumulative offsets
    let mut cumulative = 0u32;
    let mut sample_offsets: Vec<u32> = Vec::with_capacity(pixel_count);
    for pixel in &pixels {
        cumulative += pixel.sample_count() as u32;
        sample_offsets.push(cumulative);
    }
    
    // Build channel data (RGBA + Z)
    // Channels: R, G, B, A, Z
    let mut r_data: Vec<f32> = Vec::with_capacity(total_samples);
    let mut g_data: Vec<f32> = Vec::with_capacity(total_samples);
    let mut b_data: Vec<f32> = Vec::with_capacity(total_samples);
    let mut a_data: Vec<f32> = Vec::with_capacity(total_samples);
    let mut z_data: Vec<f32> = Vec::with_capacity(total_samples);
    
    for pixel in &pixels {
        for i in 0..pixel.sample_count() {
            let color = pixel.colors[i];
            r_data.push(color[0]);
            g_data.push(color[1]);
            b_data.push(color[2]);
            a_data.push(color[3]);
            z_data.push(pixel.depths[i]);
        }
    }
    
    // Create DeepSamples
    let mut samples = DeepSamples::new(width, height);
    samples.sample_offsets = sample_offsets;
    samples.channels = vec![
        DeepChannelData::F32(a_data),  // A - must be first for proper compositing
        DeepChannelData::F32(b_data),  // B
        DeepChannelData::F32(g_data),  // G
        DeepChannelData::F32(r_data),  // R
        DeepChannelData::F32(z_data),  // Z
    ];
    
    // Channel list (alphabetical order as per EXR spec)
    let channel_list = ChannelList::new(smallvec::smallvec![
        ChannelDescription::new("A", SampleType::F32, false),
        ChannelDescription::new("B", SampleType::F32, false),
        ChannelDescription::new("G", SampleType::F32, false),
        ChannelDescription::new("R", SampleType::F32, false),
        ChannelDescription::new("Z", SampleType::F32, false),
    ]);
    
    // Write using the deep write API
    write_deep_scanlines_to_file(
        path,
        &samples,
        &channel_list,
        Compression::ZIP1,
    )?;
    
    Ok(())
}

fn deep_type_name(dt: &DeepType) -> &'static str {
    match dt {
        DeepType::Particles { .. } => "particles",
        DeepType::Fog { .. } => "fog",
        DeepType::Cloud { .. } => "cloud",
        DeepType::Glass => "glass",
        DeepType::GradientVolume { .. } => "gradient-volume",
        DeepType::Explosion { .. } => "explosion",
    }
}

/// Parse size string like "1920x1080" or "2k" or "4k".
pub fn parse_size(s: &str) -> Option<(usize, usize)> {
    let s = s.to_lowercase();

    // Presets
    match s.as_str() {
        "1k" => return Some((1024, 1024)),
        "2k" => return Some((2048, 1080)),
        "4k" | "uhd" => return Some((3840, 2160)),
        "8k" => return Some((7680, 4320)),
        "hd" | "720p" => return Some((1280, 720)),
        "fhd" | "1080p" => return Some((1920, 1080)),
        "qhd" | "1440p" => return Some((2560, 1440)),
        _ => {}
    }

    // WxH format
    if let Some((w, h)) = s.split_once('x') {
        let w: usize = w.trim().parse().ok()?;
        let h: usize = h.trim().parse().ok()?;
        return Some((w, h));
    }

    // Single number = square
    if let Ok(n) = s.parse::<usize>() {
        return Some((n, n));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_size() {
        assert_eq!(parse_size("1920x1080"), Some((1920, 1080)));
        assert_eq!(parse_size("4k"), Some((3840, 2160)));
        assert_eq!(parse_size("512"), Some((512, 512)));
        assert_eq!(parse_size("1k"), Some((1024, 1024)));
    }

    #[test]
    fn test_pattern_parse() {
        assert!(PatternType::parse("gradient-h").is_some());
        assert!(PatternType::parse("gh").is_some());
        assert!(PatternType::parse("noise:8").is_some());
        assert!(PatternType::parse("checker:16").is_some());
    }

    #[test]
    fn test_shape_parse() {
        assert!(ShapeType::parse("sphere").is_some());
        assert!(ShapeType::parse("sp").is_some());
        assert!(ShapeType::parse("terrain:4:42").is_some());
    }

    #[test]
    fn test_deep_parse() {
        assert!(DeepType::parse("particles:1000").is_some());
        assert!(DeepType::parse("fog:16").is_some());
        assert!(DeepType::parse("cloud").is_some());
    }
}
