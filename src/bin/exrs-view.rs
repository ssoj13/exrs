//! EXR image viewer.
//!
//! Usage:
//!   exrs-view [OPTIONS] [FILE.exr]
//!
//! Options:
//!   -v, --verbose    Verbose output
//!   -h, --help       Show help
//!   -V, --version    Show version

use std::env;
use std::process::ExitCode;

use exr::view::{run, run_empty, ViewerConfig};

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();

    if args.iter().any(|a| a == "-h" || a == "--help") {
        print_help();
        return ExitCode::SUCCESS;
    }

    if args.iter().any(|a| a == "-V" || a == "--version") {
        println!("exrs-view {VERSION}");
        return ExitCode::SUCCESS;
    }

    let verbose = args.iter().any(|a| a == "-v" || a == "--verbose");

    // Find file argument (first non-flag argument after program name)
    let file_path = args
        .iter()
        .skip(1)
        .find(|a| !a.starts_with('-'))
        .map(|s| s.to_string());

    let config = ViewerConfig {
        verbose: if verbose { 1 } else { 0 },
    };

    let exit_code = if let Some(path) = file_path {
        run(&path, config)
    } else {
        run_empty(config)
    };

    ExitCode::from(exit_code as u8)
}

fn print_help() {
    println!(
        r#"
exrs-view - EXR Image Viewer v{VERSION}

USAGE:
    exrs-view [OPTIONS] [FILE.exr]

OPTIONS:
    -v, --verbose    Verbose output
    -h, --help       Show this help
    -V, --version    Show version

FEATURES:
    - Multi-layer EXR support
    - Channel selection (R/G/B/A/Z/L/custom)
    - Deep data visualization
      - Flattened (over composite)
      - Sample count heatmap
      - Depth slice
      - First/last sample
    - Depth normalization
      - Auto (min-max)
      - Manual range
      - Logarithmic scale
    - Exposure control (EV stops)
    - sRGB gamma toggle
    - Zoom/pan (scroll wheel, drag)
    - Drag & drop support

KEYBOARD SHORTCUTS:
    R/G/B/A/Z  Channel modes
    C          Color mode
    L          Luminance
    F          Fit to window
    H          Home (1:1 zoom)
    +/-        Zoom in/out
    Ctrl+O     Open file
    Esc        Exit

EXAMPLES:
    exrs-view image.exr
    exrs-view -v render.exr
    exrs-view                    # Opens empty, use Ctrl+O or drag & drop
"#
    );
}
