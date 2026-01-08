//! Viewer state types.

use std::path::PathBuf;

/// Channel display mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChannelMode {
    /// Full color (RGB/RGBA).
    #[default]
    Color,
    /// Red channel only.
    Red,
    /// Green channel only.
    Green,
    /// Blue channel only.
    Blue,
    /// Alpha channel only.
    Alpha,
    /// Z/Depth channel.
    Depth,
    /// Luminance (grayscale).
    Luminance,
    /// Custom channel by name.
    Custom(usize),
}

impl ChannelMode {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Color => "Color",
            Self::Red => "Red",
            Self::Green => "Green",
            Self::Blue => "Blue",
            Self::Alpha => "Alpha",
            Self::Depth => "Depth",
            Self::Luminance => "Luminance",
            Self::Custom(_) => "Custom",
        }
    }

    pub const fn shortcut(self) -> &'static str {
        match self {
            Self::Color => "C",
            Self::Red => "R",
            Self::Green => "G",
            Self::Blue => "B",
            Self::Alpha => "A",
            Self::Depth => "Z",
            Self::Luminance => "L",
            Self::Custom(_) => "",
        }
    }

    pub const fn all_basic() -> &'static [Self] {
        &[
            Self::Color,
            Self::Red,
            Self::Green,
            Self::Blue,
            Self::Alpha,
            Self::Depth,
            Self::Luminance,
        ]
    }
}



/// Deep data visualization mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DeepMode {
    /// Composite all samples (over operation).
    #[default]
    Flattened,
    /// Heatmap of sample count per pixel.
    SampleCount,
    /// Show samples in depth range.
    DepthSlice,
    /// First (nearest) sample only.
    FirstSample,
    /// Last (farthest) sample only.
    LastSample,
    /// Min depth per pixel.
    MinDepth,
    /// Max depth per pixel.
    MaxDepth,
}

impl DeepMode {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Flattened => "Flattened",
            Self::SampleCount => "Sample Count",
            Self::DepthSlice => "Depth Slice",
            Self::FirstSample => "First Sample",
            Self::LastSample => "Last Sample",
            Self::MinDepth => "Min Depth",
            Self::MaxDepth => "Max Depth",
        }
    }

    pub const fn all() -> &'static [Self] {
        &[
            Self::Flattened,
            Self::SampleCount,
            Self::DepthSlice,
            Self::FirstSample,
            Self::LastSample,
            Self::MinDepth,
            Self::MaxDepth,
        ]
    }
}

/// Depth normalization mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DepthMode {
    /// No normalization (raw values).
    Raw,
    /// Auto min-max normalization.
    #[default]
    AutoNormalize,
    /// Manual near/far range.
    ManualRange,
    /// Logarithmic scale.
    Logarithmic,
}

impl DepthMode {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Raw => "Raw",
            Self::AutoNormalize => "Auto",
            Self::ManualRange => "Manual",
            Self::Logarithmic => "Log",
        }
    }

    pub const fn all() -> &'static [Self] {
        &[Self::Raw, Self::AutoNormalize, Self::ManualRange, Self::Logarithmic]
    }
}

/// 3D visualization mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum View3DMode {
    /// Z-buffer as heightfield mesh.
    #[default]
    Heightfield,
    /// Deep samples as point cloud.
    PointCloud,
    /// Position pass (P.xyz channels).
    PositionPass,
}

impl View3DMode {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Heightfield => "Heightfield",
            Self::PointCloud => "Point Cloud",
            Self::PositionPass => "Position",
        }
    }

    pub const fn all() -> &'static [Self] {
        &[Self::Heightfield, Self::PointCloud, Self::PositionPass]
    }
}

/// Runtime viewer state.
#[derive(Debug, Clone)]
pub struct ViewerState {
    // Image info
    pub image_path: Option<PathBuf>,
    pub image_dims: Option<(usize, usize)>,
    pub is_deep: bool,
    pub total_samples: usize,
    pub avg_samples: f32,

    // Layer/channel selection
    pub layers: Vec<String>,
    pub current_layer: String,
    pub channels: Vec<String>,
    pub current_channel: String,

    // Display settings
    pub show_3d: bool,
    pub channel_mode: ChannelMode,
    pub deep_mode: DeepMode,
    pub depth_mode: DepthMode,
    pub view_3d_mode: View3DMode,

    // Exposure and color
    pub exposure: f32,
    pub gamma: f32,
    pub apply_srgb: bool,

    // Depth settings
    pub depth_near: f32,
    pub depth_far: f32,
    pub depth_invert: bool,
    pub depth_auto_range: (f32, f32),

    // Deep slice settings
    pub slice_near: f32,
    pub slice_far: f32,

    // View controls
    pub zoom: f32,
    pub pan: [f32; 2],
    pub viewport_size: [f32; 2],

    // 3D camera
    pub camera_yaw: f32,
    pub camera_pitch: f32,
    pub camera_distance: f32,
    pub point_size: f32,

    // Error display
    pub error: Option<String>,
}

impl Default for ViewerState {
    fn default() -> Self {
        Self {
            image_path: None,
            image_dims: None,
            is_deep: false,
            total_samples: 0,
            avg_samples: 0.0,

            layers: Vec::new(),
            current_layer: String::new(),
            channels: Vec::new(),
            current_channel: String::new(),

            show_3d: false,
            channel_mode: ChannelMode::Color,
            deep_mode: DeepMode::Flattened,
            depth_mode: DepthMode::AutoNormalize,
            view_3d_mode: View3DMode::Heightfield,

            exposure: 0.0,
            gamma: 2.2,
            apply_srgb: true,

            depth_near: 0.0,
            depth_far: 1.0,
            depth_invert: false,
            depth_auto_range: (0.0, 1.0),

            slice_near: 0.0,
            slice_far: 1.0,

            zoom: 1.0,
            pan: [0.0, 0.0],
            viewport_size: [1280.0, 720.0],

            camera_yaw: 0.0,
            camera_pitch: 0.3,
            camera_distance: 2.0,
            point_size: 2.0,

            error: None,
        }
    }
}
