# Quick Start

Get up and running with exrs in minutes.

## Installation

Add exrs to your `Cargo.toml`:

```toml
[dependencies]
exr = "1.74.0"
```

For best performance, enable link-time optimization:

```toml
[profile.release]
lto = true
```

## The Prelude

Import the prelude for convenient access to common types:

```rust
use exr::prelude::*;
```

This includes:
- `read()`, `write_rgba_file()`, etc.
- `Image`, `Layer`, `Encoding`
- `f16`, `Vec2`, `Text`
- `Error`, `Result`

## Writing Your First EXR

### Simple RGBA

The easiest way to write an image:

```rust
use exr::prelude::*;

fn main() {
    write_rgba_file(
        "my_first.exr",
        800, 600,  // width, height
        |x, y| {
            // Return (R, G, B, A) for each pixel
            let r = x as f32 / 800.0;
            let g = y as f32 / 600.0;
            let b = 0.5_f32;
            let a = 1.0_f32;
            (r, g, b, a)
        }
    ).expect("Failed to write EXR");
    
    println!("Created my_first.exr");
}
```

### RGB Without Alpha

```rust
use exr::prelude::*;

fn main() {
    write_rgb_file(
        "rgb_only.exr",
        1920, 1080,
        |x, y| {
            (0.2_f32, 0.4_f32, 0.6_f32)
        }
    ).unwrap();
}
```

## Reading Your First EXR

### Read RGBA

```rust
use exr::prelude::*;

fn main() {
    let image = read_first_rgba_layer_from_file(
        "my_first.exr",
        
        // Constructor: create your pixel storage
        |resolution, _channels| {
            vec![(0.0_f32, 0.0, 0.0, 1.0); resolution.width() * resolution.height()]
        },
        
        // Setter: store each pixel
        |pixels, position, (r, g, b, a): (f32, f32, f32, f32)| {
            let index = position.y() * position.width() + position.x();
            pixels[index] = (r, g, b, a);
        }
    ).expect("Failed to read EXR");
    
    println!("Loaded image: {}x{}", 
        image.layer_data.size.0, 
        image.layer_data.size.1
    );
}
```

### Read All Data (Dynamic)

When you don't know what's in the file:

```rust
use exr::prelude::*;

fn main() {
    let image = read_all_data_from_file("unknown.exr")
        .expect("Failed to read EXR");
    
    println!("Image has {} layers", image.layer_data.len());
    
    for (i, layer) in image.layer_data.iter().enumerate() {
        println!("Layer {}: {} channels, {}x{}", 
            i,
            layer.channel_data.list.len(),
            layer.size.0,
            layer.size.1
        );
        
        for channel in &layer.channel_data.list {
            println!("  - {} ({:?})", 
                channel.name, 
                channel.sample_data.sample_type()
            );
        }
    }
}
```

## Working with Pixel Data

### From Existing Data

If you already have pixel data:

```rust
use exr::prelude::*;

fn main() {
    // Your existing pixel buffer
    let width = 256;
    let height = 256;
    let pixels: Vec<(f32, f32, f32, f32)> = (0..width*height)
        .map(|i| {
            let x = (i % width) as f32 / width as f32;
            let y = (i / width) as f32 / height as f32;
            (x, y, 0.5, 1.0)
        })
        .collect();
    
    // Write using closure that looks up your data
    write_rgba_file(
        "from_buffer.exr",
        width, height,
        |x, y| pixels[y * width + x]
    ).unwrap();
}
```

### Using half (f16)

For smaller files, use 16-bit floats:

```rust
use exr::prelude::*;

fn main() {
    write_rgba_file(
        "half_precision.exr",
        1920, 1080,
        |x, y| {
            // Return f16 values for half-precision storage
            (
                f16::from_f32(x as f32 / 1920.0),
                f16::from_f32(y as f32 / 1080.0),
                f16::from_f32(0.5),
                f16::ONE
            )
        }
    ).unwrap();
}
```

## Compression Options

### Quick Presets

```rust
use exr::prelude::*;

// In Layer construction:
let layer = Layer::new(
    (1920, 1080),
    LayerAttributes::named("main"),
    Encoding::FAST_LOSSLESS,  // ZIP, good balance
    channels
);

// Other presets:
// Encoding::UNCOMPRESSED     - Fastest I/O, largest files
// Encoding::SMALL_LOSSLESS   - PIZ, best compression
// Encoding::SMALL_LOSSY      - B44, smallest lossy
```

### Custom Compression

```rust
use exr::prelude::*;

let encoding = Encoding {
    compression: Compression::ZIP16,  // ZIP with 16-line blocks
    blocks: Blocks::ScanLines,        // Scanline-based
    line_order: LineOrder::Increasing,
};
```

## Error Handling

exrs uses a custom `Error` type:

```rust
use exr::prelude::*;

fn main() {
    match read_all_data_from_file("maybe_missing.exr") {
        Ok(image) => println!("Loaded successfully"),
        Err(Error::Io(io_err)) => {
            eprintln!("File error: {}", io_err);
        }
        Err(Error::Invalid(msg)) => {
            eprintln!("Invalid EXR: {}", msg);
        }
        Err(Error::NotSupported(msg)) => {
            eprintln!("Unsupported feature: {}", msg);
        }
        Err(e) => eprintln!("Other error: {:?}", e),
    }
}
```

## Parallel Processing

By default, exrs uses all CPU cores for compression/decompression. To disable:

```rust
use exr::prelude::*;

// Reading
let image = read()
    .no_deep_data()
    .largest_resolution_level()
    .all_channels()
    .first_valid_layer()
    .all_attributes()
    .non_parallel()  // Single-threaded
    .from_file("image.exr")
    .unwrap();

// Writing
image.write()
    .non_parallel()  // Single-threaded
    .to_file("output.exr")
    .unwrap();
```

## Progress Callbacks

For large files, track progress:

```rust
use exr::prelude::*;

fn main() {
    // Reading with progress
    let image = read()
        .no_deep_data()
        .largest_resolution_level()
        .all_channels()
        .first_valid_layer()
        .all_attributes()
        .on_progress(|progress| {
            print!("\rReading: {:.1}%", progress * 100.0);
        })
        .from_file("large.exr")
        .unwrap();
    println!("\nDone!");
    
    // Writing with progress
    image.write()
        .on_progress(|progress| {
            print!("\rWriting: {:.1}%", progress * 100.0);
        })
        .to_file("output.exr")
        .unwrap();
    println!("\nDone!");
}
```

## What's Next?

- [Reading Images](./reading.md) - Advanced reading options
- [Writing Images](./writing.md) - Custom layers and channels
- [Deep Data](./deep-data.md) - Variable samples per pixel
- [Our Enhancements](./our-enhancements/overview.md) - Extended features
