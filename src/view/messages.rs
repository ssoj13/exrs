//! Message types for UI <-> Worker communication.

use std::path::PathBuf;
use egui::Color32;

use crate::view::state::{ChannelMode, DeepMode, DepthMode, View3DMode};

/// Generation counter for invalidating stale results.
pub type Generation = u64;

/// Messages from UI to worker thread.
#[derive(Debug, Clone)]
pub enum ViewerMsg {
    /// Load an EXR file.
    LoadImage(PathBuf),

    /// Set current layer.
    SetLayer(String),

    /// Set current channel.
    SetChannel(String),

    /// Set channel mode.
    SetChannelMode(ChannelMode),

    /// Set deep visualization mode.
    SetDeepMode(DeepMode),

    /// Set depth normalization mode.
    SetDepthMode(DepthMode),

    /// Set depth range (near, far).
    SetDepthRange(f32, f32),

    /// Set depth slice range for deep data.
    SetSliceRange(f32, f32),

    /// Set exposure (EV stops).
    SetExposure(f32),

    /// Toggle sRGB gamma.
    SetSrgb(bool),

    /// Set invert depth.
    SetInvertDepth(bool),

    /// Regenerate texture.
    Regenerate,

    /// Sync generation counter.
    SyncGeneration(Generation),

    /// Zoom by factor.
    Zoom { factor: f32 },

    /// Pan by delta.
    Pan { delta: [f32; 2] },

    /// Fit to window.
    FitToWindow,

    /// Reset view (1:1).
    Home,

    /// Set viewport size.
    SetViewport([f32; 2]),

    /// Close viewer.
    Close,
    
    /// Request 3D depth data for visualization.
    Request3DData,
    
    /// Set 3D visualization mode.
    Set3DMode(View3DMode),
    
    /// Set point size for 3D point cloud.
    SetPointSize(f32),
    
    /// Reset 3D camera to default position.
    Reset3DCamera,
    
    /// Toggle 3D panel visibility.
    Toggle3D(bool),
}

/// Events from worker to UI thread.
#[derive(Debug)]
pub enum ViewerEvent {
    /// Image loaded successfully.
    ImageLoaded {
        path: PathBuf,
        dims: (usize, usize),
        layers: Vec<String>,
        channels: Vec<String>,
        is_deep: bool,
        total_samples: usize,
        depth_range: Option<(f32, f32)>,
    },

    /// Texture ready for display.
    TextureReady {
        generation: Generation,
        width: usize,
        height: usize,
        pixels: Vec<Color32>,
    },

    /// View state sync.
    StateSync {
        zoom: f32,
        pan: [f32; 2],
    },

    /// Error occurred.
    Error(String),
    
    /// 3D depth data ready.
    Data3DReady {
        width: usize,
        height: usize,
        depth: Vec<f32>,
    },
}
