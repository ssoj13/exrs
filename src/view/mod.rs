//! EXR image viewer with 2D and 3D visualization.
//!
//! Features:
//! - Multi-layer EXR support with layer/channel selection
//! - Deep data visualization (sample count, flattened, depth slice)
//! - Depth normalization (auto, manual range, log scale)
//! - Exposure control, zoom/pan
//! - 3D mode: heightfield, point cloud (with view-3d feature)
//!
//! # Quick Start
//!
//! ```ignore
//! use exr::view::{run, ViewerConfig};
//!
//! let config = ViewerConfig::default();
//! run("image.exr", config);
//! ```

#![allow(missing_docs)]
#![allow(missing_copy_implementations)]

mod app;
mod handler;
mod messages;
mod state;

#[cfg(feature = "view-3d")]
mod view3d;

pub use app::{ViewerApp, ViewerConfig};
pub use state::{ChannelMode, DeepMode, DepthMode, ViewerState};

use std::path::Path;

/// Run the EXR viewer with an image file.
///
/// Creates a window and enters the event loop.
/// Returns exit code (0 = success, 1 = error).
pub fn run<P: AsRef<Path>>(path: P, config: ViewerConfig) -> i32 {
    let path = path.as_ref();

    if !path.exists() {
        eprintln!("Error: File not found: {}", path.display());
        return 1;
    }

    let title = format!(
        "exrs view - {}",
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("EXR Viewer")
    );

    run_internal(Some(path.to_path_buf()), title, config)
}

/// Run the viewer without an initial file.
pub fn run_empty(config: ViewerConfig) -> i32 {
    run_internal(None, "exrs view".into(), config)
}

fn run_internal(
    path: Option<std::path::PathBuf>,
    title: String,
    config: ViewerConfig,
) -> i32 {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(&title)
            .with_inner_size([1400.0, 900.0])
            .with_min_inner_size([800.0, 600.0]),
        ..Default::default()
    };

    let result = eframe::run_native(
        &title,
        native_options,
        Box::new(move |cc| Ok(Box::new(ViewerApp::new(cc, path, config)))),
    );

    match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("Viewer error: {e}");
            1
        }
    }
}
