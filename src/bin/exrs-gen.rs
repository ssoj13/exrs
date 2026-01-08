//! EXR test image generator CLI.
//!
//! Generate test EXR files with various patterns, shapes, and deep data.

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use exr::gen::{self, PatternType, ShapeType, DeepType, ChannelSpec};

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();

    // Handle subcommands
    if args.len() >= 2 {
        match args[1].as_str() {
            "view" => return run_viewer(&args[2..]),
            "gen" => {
                // Explicit gen command - shift args
                let shifted: Vec<String> = std::iter::once(args[0].clone())
                    .chain(args[2..].iter().cloned())
                    .collect();
                return run_generator(&shifted);
            }
            _ => {}
        }
    }

    run_generator(&args)
}

#[cfg(feature = "view")]
fn run_viewer(args: &[String]) -> ExitCode {
    use exr::view::{run, run_empty, ViewerConfig};
    
    // Parse viewer args
    let mut verbose = 0u8;
    let mut file: Option<PathBuf> = None;
    
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print_view_help();
                return ExitCode::SUCCESS;
            }
            "-v" | "--verbose" => verbose = 1,
            "-vv" => verbose = 2,
            arg if !arg.starts_with('-') => file = Some(PathBuf::from(arg)),
            _ => {
                eprintln!("Unknown viewer option: {}", args[i]);
                return ExitCode::FAILURE;
            }
        }
        i += 1;
    }
    
    let config = ViewerConfig { verbose };
    
    let exit_code = match file {
        Some(path) => run(path, config),
        None => run_empty(config),
    };
    
    if exit_code == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

#[cfg(not(feature = "view"))]
fn run_viewer(_args: &[String]) -> ExitCode {
    eprintln!("Error: Viewer not available. Rebuild with --features view");
    ExitCode::FAILURE
}

fn run_generator(args: &[String]) -> ExitCode {
    if args.len() < 2 || args.iter().any(|a| a == "-h" || a == "--help") {
        print_help();
        return ExitCode::SUCCESS;
    }

    if args.iter().any(|a| a == "-V" || a == "--version") {
        println!("exrs-gen {VERSION}");
        return ExitCode::SUCCESS;
    }

    if args.iter().any(|a| a == "-l" || a == "--list") {
        print_list();
        return ExitCode::SUCCESS;
    }

    // Parse arguments
    let mut output: Option<String> = None;
    let mut size = (1024usize, 1024usize);
    let mut pattern = PatternType::GradientRadial;
    let mut shape: Option<ShapeType> = None;
    let mut channels = ChannelSpec::Rgba;
    let mut deep: Option<DeepType> = None;
    let mut seed: Option<u32> = None;

    let mut i = 1;
    while i < args.len() {
        let arg = &args[i];
        match arg.as_str() {
            "-s" | "--size" => {
                i += 1;
                if i < args.len() {
                    if let Some(s) = gen::parse_size(&args[i]) {
                        size = s;
                    } else {
                        eprintln!("Error: Invalid size '{}'", args[i]);
                        return ExitCode::FAILURE;
                    }
                }
            }
            "-p" | "--pattern" => {
                i += 1;
                if i < args.len() {
                    if let Some(p) = PatternType::parse(&args[i]) {
                        pattern = p;
                    } else {
                        eprintln!("Error: Unknown pattern '{}'. Use --list for options.", args[i]);
                        return ExitCode::FAILURE;
                    }
                }
            }
            "-z" | "--zshape" | "--shape" => {
                i += 1;
                if i < args.len() {
                    if let Some(s) = ShapeType::parse(&args[i]) {
                        shape = Some(s);
                    } else {
                        eprintln!("Error: Unknown shape '{}'. Use --list for options.", args[i]);
                        return ExitCode::FAILURE;
                    }
                }
            }
            "-c" | "--channels" => {
                i += 1;
                if i < args.len() {
                    if let Some(c) = ChannelSpec::parse(&args[i]) {
                        channels = c;
                    } else {
                        eprintln!("Error: Unknown channel spec '{}'. Use --list for options.", args[i]);
                        return ExitCode::FAILURE;
                    }
                }
            }
            "-d" | "--deep" => {
                i += 1;
                if i < args.len() {
                    if let Some(d) = DeepType::parse(&args[i]) {
                        deep = Some(d);
                    } else {
                        eprintln!("Error: Unknown deep type '{}'. Use --list for options.", args[i]);
                        return ExitCode::FAILURE;
                    }
                }
            }
            "-r" | "--random" | "--seed" => {
                i += 1;
                if i < args.len() {
                    if let Ok(s) = args[i].parse() {
                        seed = Some(s);
                    }
                } else {
                    // Random seed from time
                    seed = Some(std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u32);
                }
            }
            "-o" | "--output" => {
                i += 1;
                if i < args.len() {
                    output = Some(args[i].clone());
                }
            }
            _ if !arg.starts_with('-') => {
                // Positional argument = output file
                output = Some(arg.clone());
            }
            _ => {
                eprintln!("Error: Unknown option '{arg}'");
                return ExitCode::FAILURE;
            }
        }
        i += 1;
    }

    // Apply seed if specified
    if let Some(s) = seed {
        pattern = match pattern {
            PatternType::NoisePerlin { freq, .. } => PatternType::NoisePerlin { freq, seed: s },
            PatternType::NoiseFbm { freq, octaves, .. } => PatternType::NoiseFbm { freq, octaves, seed: s },
            PatternType::NoiseRidged { freq, octaves, .. } => PatternType::NoiseRidged { freq, octaves, seed: s },
            PatternType::NoiseVoronoi { freq, .. } => PatternType::NoiseVoronoi { freq, seed: s },
            other => other,
        };
        shape = shape.map(|sh| match sh {
            ShapeType::Terrain { freq, .. } => ShapeType::Terrain { freq, seed: s },
            ShapeType::Mountains { freq, .. } => ShapeType::Mountains { freq, seed: s },
            ShapeType::Cells { freq, .. } => ShapeType::Cells { freq, seed: s },
            other => other,
        });
        deep = deep.map(|d| match d {
            DeepType::Particles { count, .. } => DeepType::Particles { count, seed: s },
            DeepType::Cloud { samples, .. } => DeepType::Cloud { samples, seed: s },
            DeepType::Explosion { .. } => DeepType::Explosion { seed: s },
            other => other,
        });
    }

    // Output file
    let output = output.unwrap_or_else(|| "output.exr".to_string());

    // Generate
    let start = Instant::now();

    if let Some(ref deep_type) = deep {
        // Deep data generation
        println!("Generating DEEP {}x{} {} -> {}", size.0, size.1, deep_name(deep_type), output);
        
        if let Err(e) = gen::generate_deep(&output, size.0, size.1, deep_type) {
            eprintln!("Error: {e}");
            return ExitCode::FAILURE;
        }
    } else {
        // Flat (non-deep) generation
        println!("Generating {}x{} {} -> {}", size.0, size.1, pattern_name(&pattern), output);

        if let Err(e) = gen::generate_flat(&output, size.0, size.1, &pattern, shape.as_ref(), &channels) {
            eprintln!("Error: {e}");
            return ExitCode::FAILURE;
        }
    }

    let elapsed = start.elapsed();
    println!("Done in {:.2}s", elapsed.as_secs_f32());

    ExitCode::SUCCESS
}

fn deep_name(d: &DeepType) -> &'static str {
    match d {
        DeepType::Particles { .. } => "particles",
        DeepType::Fog { .. } => "fog",
        DeepType::Cloud { .. } => "cloud",
        DeepType::Glass => "glass",
        DeepType::GradientVolume { .. } => "gradient-volume",
        DeepType::Explosion { .. } => "explosion",
    }
}

fn pattern_name(p: &PatternType) -> &'static str {
    match p {
        PatternType::GradientH => "gradient-h",
        PatternType::GradientV => "gradient-v",
        PatternType::GradientRadial => "gradient-radial",
        PatternType::GradientAngular => "gradient-angular",
        PatternType::Checker { .. } => "checker",
        PatternType::Grid { .. } => "grid",
        PatternType::Dots { .. } => "dots",
        PatternType::NoisePerlin { .. } => "noise-perlin",
        PatternType::NoiseFbm { .. } => "noise-fbm",
        PatternType::NoiseRidged { .. } => "noise-ridged",
        PatternType::NoiseVoronoi { .. } => "noise-voronoi",
        PatternType::Waves { .. } => "waves",
        PatternType::Ripples { .. } => "ripples",
        PatternType::ZonePlate { .. } => "zoneplate",
        PatternType::ColorBars => "colorbars",
        PatternType::UvMap => "uvmap",
        PatternType::Solid { .. } => "solid",
    }
}

fn print_help() {
    println!(r#"
exrs-gen - EXR Test Image Generator v{VERSION}

USAGE:
    exrs-gen [OPTIONS] <OUTPUT.exr>
    exrs-gen view [FILE.exr]         View EXR files
    exrs-gen --list

OPTIONS:
    -s, --size <WxH>      Image size (default: 1024x1024)
                          Presets: 1k, 2k, 4k, 8k, hd, fhd, qhd
    -p, --pattern <NAME>  Pattern type (default: gradient-radial)
    -z, --zshape <NAME>   Z-depth shape (enables Z channel)
    -c, --channels <SPEC> Channel configuration (default: rgba)
    -d, --deep <TYPE>     Deep data type (enables deep EXR)
    -r, --seed <N>        Random seed for noise patterns
    -o, --output <FILE>   Output file (or positional arg)
    -l, --list            List all available patterns/shapes
    -h, --help            Show this help
    -V, --version         Show version

EXAMPLES:
  Patterns:
    exrs-gen -s 1k -p gradient-radial gradient.exr
    exrs-gen -s 2k -p fbm:4:6 -c rgb noise.exr
    exrs-gen -s fhd -p colorbars test.exr
    exrs-gen -s 1k -p checker:16 checker.exr

  Z-Depth (flat EXR with depth channel):
    exrs-gen -s 1k -z sphere -c rgbaz sphere.exr
    exrs-gen -s 2k -z terrain:4 -c z heightmap.exr
    exrs-gen -s 1k -z mountains:3 -p ridged:3 -c rgbaz terrain.exr
    exrs-gen -s 1k -z waves:8 -c rgbz waves.exr

  Deep Data (variable samples per pixel):
    exrs-gen -s 512 -d fog:8 deep_fog.exr
    exrs-gen -s 256 -d particles:1000 deep_particles.exr
    exrs-gen -s 512 -d cloud:16 -r 42 deep_cloud.exr
    exrs-gen -s 256 -d explosion deep_explosion.exr

  Viewer:
    exrs-gen view                        # Open file dialog
    exrs-gen view image.exr              # View specific file
    exrs-gen -s 1k -d fog:4 fog.exr && exrs-gen view fog.exr

SHORT ALIASES:
    Patterns: gh gv gr ga ch np nf nr nv w rp zp cb uv s
    Shapes:   sp bx pl cn cyl tor ter mtn ws ms
    Deep:     p(articles) f(og) cl(oud) gl(ass) gv(gradient-vol) e(xplosion)

Use --list for complete pattern/shape/channel reference.
Use 'exrs-gen view --help' for viewer options.
"#);
}

fn print_view_help() {
    println!(r#"
exrs-gen view - EXR Viewer v{VERSION}

USAGE:
    exrs-gen view [OPTIONS] [FILE.exr]

OPTIONS:
    -v, --verbose    Verbose output
    -vv              Extra verbose
    -h, --help       Show this help

KEYBOARD:
    R/G/B/A/Z        Show single channel
    C                Show color (RGB)
    L                Show luminance
    F                Fit to window
    H                Reset view (1:1)
    +/-              Zoom in/out
    Ctrl+O           Open file dialog
    Esc/Q            Quit

DEEP DATA:
    When viewing deep EXR files, additional modes are available
    in the control panel: Flattened, Sample Count, Depth Slice,
    First/Last Sample, Min/Max Depth.

DEPTH NORMALIZATION:
    For depth channels (Z), use the depth mode controls to
    normalize the display range: Auto, Manual, or Logarithmic.

EXAMPLES:
    exrs-gen view                     # Empty window, double-click to open
    exrs-gen view render.exr          # View specific file
    exrs-gen view deep_fog.exr        # View deep data (use Deep Mode menu)

    # Generate and view in one command:
    exrs-gen -s 512 -d fog:8 fog.exr && exrs-gen view fog.exr

TIPS:
    - Double-click empty area or drag-drop to open files
    - Use Z key to view depth channel, then adjust Depth Mode
    - For deep data: try Sample Count mode to see sample density
"#);
}

fn print_list() {
    println!("PATTERNS (2D):");
    for p in PatternType::list() {
        println!("  {p}");
    }

    println!("\nZ-DEPTH SHAPES:");
    for s in ShapeType::list() {
        println!("  {s}");
    }

    println!("\nDEEP DATA TYPES:");
    for d in DeepType::list() {
        println!("  {d}");
    }

    println!("\nCHANNEL SPECS:");
    for c in ChannelSpec::list() {
        println!("  {c}");
    }

    println!("\nSIZE PRESETS:");
    println!("  1k=1024x1024  2k=2048x1080  4k=3840x2160  8k=7680x4320");
    println!("  hd=1280x720   fhd=1920x1080 qhd=2560x1440");
}
