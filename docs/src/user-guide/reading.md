# Reading Images

This guide covers all the ways to read EXR files with exrs.

## Reading Pipeline Overview

Reading an EXR file follows a builder pattern:

```rust
use exr::prelude::*;

let image = read()              // Start builder
    .no_deep_data()             // Skip deep data
    .largest_resolution_level() // Only full resolution
    .all_channels()             // Load all channels
    .first_valid_layer()        // First matching layer
    .all_attributes()           // Keep all metadata
    .from_file("image.exr")?;   // Execute
```

Each method refines what data you want, and the final `from_file()` / `from_buffered()` executes the read.

## Convenience Functions

For common cases, use these one-liners:

```rust
use exr::prelude::*;

// RGBA with custom storage
let image = read_first_rgba_layer_from_file(
    "image.exr",
    |res, _| vec![(0f32, 0f32, 0f32, 1f32); res.width() * res.height()],
    |pixels, pos, px| pixels[pos.flat_index()] = px
)?;

// All RGBA layers
let image = read_all_rgba_layers_from_file(
    "multilayer.exr",
    |res, _| vec![(0f32, 0f32, 0f32, 1f32); res.width() * res.height()],
    |pixels, pos, px| pixels[pos.flat_index()] = px
)?;

// First layer, any channels
let image = read_first_flat_layer_from_file("image.exr")?;

// All layers, all channels (most flexible)
let image = read_all_flat_layers_from_file("image.exr")?;

// Everything including metadata
let image = read_all_data_from_file("image.exr")?;
```

## Deep vs Flat Data

The first choice: do you want deep data (variable samples per pixel)?

```rust
use exr::prelude::*;

// Flat data only (one sample per pixel)
let reader = read().no_deep_data();

// Deep data only
use exr::image::read::deep::read_deep;
let reader = read_deep();

// Either (unified reader)
let image = read_first_any_layer_from_file("unknown.exr")?;
```

See [Deep Data](./deep-data.md) for more on deep images.

## Resolution Levels

EXR files may contain mip maps or rip maps:

```rust
use exr::prelude::*;

// Only the largest (full) resolution
let reader = read().no_deep_data().largest_resolution_level();

// All resolution levels
let reader = read().no_deep_data().all_resolution_levels();

// Specific level by index
let reader = read().no_deep_data()
    .specific_resolution_level(|levels| {
        // levels: Vec<LevelInfo> with index and resolution
        Vec2(1, 1)  // Return desired level index
    });

// Level closest to target size
let reader = read().no_deep_data()
    .specific_resolution_level(|levels| {
        let target = 512;
        levels.iter()
            .min_by_key(|l| (l.resolution.x() as i64 - target).abs())
            .map(|l| l.index)
            .unwrap_or(Vec2(0, 0))
    });
```

## Channel Selection

### All Channels

Load every channel as a dynamic list:

```rust
use exr::prelude::*;

let reader = read()
    .no_deep_data()
    .largest_resolution_level()
    .all_channels();  // Vec<AnyChannel>
```

### RGBA Channels

Load specifically R, G, B, A (with A optional):

```rust
use exr::prelude::*;

let image = read()
    .no_deep_data()
    .largest_resolution_level()
    .rgba_channels(
        // Constructor
        |resolution, _channel_info| {
            MyImage::new(resolution.width(), resolution.height())
        },
        // Pixel setter
        |image: &mut MyImage, position, (r, g, b, a): (f32, f32, f32, f32)| {
            image.set_pixel(position.x(), position.y(), r, g, b, a);
        }
    )
    .first_valid_layer()
    .all_attributes()
    .from_file("image.exr")?;
```

### RGB Channels

Same as RGBA but without alpha:

```rust
.rgb_channels(
    |resolution, _| MyImage::new(resolution.width(), resolution.height()),
    |image, pos, (r, g, b): (f32, f32, f32)| {
        image.set_rgb(pos.x(), pos.y(), r, g, b);
    }
)
```

### Specific Channels by Name

Load exactly the channels you need:

```rust
use exr::prelude::*;

let image = read()
    .no_deep_data()
    .largest_resolution_level()
    .specific_channels()
        .required("R")
        .required("G") 
        .required("B")
        .optional("depth", 1.0_f32)  // Default if missing
        .collect_pixels(
            // Constructor
            |res, _| vec![(0f32, 0f32, 0f32, 0f32); res.width() * res.height()],
            // Setter - tuple matches channel order
            |vec, pos, (r, g, b, depth): (f32, f32, f32, f32)| {
                vec[pos.flat_index()] = (r, g, b, depth);
            }
        )
    .first_valid_layer()
    .all_attributes()
    .from_file("render.exr")?;
```

### Pixel Type Conversion

The setter's pixel type determines conversion:

```rust
// Read as f16 (native for most EXR)
|vec, pos, (r, g, b, a): (f16, f16, f16, f16)| { ... }

// Read as f32 (automatic conversion)
|vec, pos, (r, g, b, a): (f32, f32, f32, f32)| { ... }

// Keep original type (no conversion)
|vec, pos, (r, g, b, a): (Sample, Sample, Sample, Sample)| { ... }
```

## Layer Selection

### First Valid Layer

Get the first layer matching your channel requirements:

```rust
.first_valid_layer()
```

### All Layers

Get all layers as a `Vec`:

```rust
.all_layers()
```

### All Valid Layers

Get all layers matching your channel requirements (skips incompatible layers):

```rust
let image = read()
    .no_deep_data()
    .largest_resolution_level()
    .rgb_channels(...)
    .all_valid_layers()  // Only layers with RGB
    .all_attributes()
    .from_file("mixed.exr")?;

if image.layer_data.is_empty() {
    println!("No RGB layers found");
}
```

## Metadata

Currently, you must load all attributes:

```rust
.all_attributes()
```

Access them after loading:

```rust
let image = read_all_data_from_file("image.exr")?;

// Image-level attributes
let display = &image.attributes.display_window;
println!("Display window: {:?}", display);

// Layer-level attributes
for layer in &image.layer_data {
    if let Some(name) = &layer.attributes.layer_name {
        println!("Layer: {}", name);
    }
    
    // Custom attributes
    for (key, value) in &layer.attributes.other {
        println!("  {}: {:?}", key, value);
    }
}
```

## Reading Options

### Progress Callback

```rust
.on_progress(|progress: f64| {
    println!("{}% complete", (progress * 100.0) as i32);
})
```

### Non-Parallel

Disable multi-threaded decompression:

```rust
.non_parallel()
```

### Pedantic Mode

Strict validation (rejects some valid-but-unusual files):

```rust
use exr::image::read::ReadOptions;

// Via builder (if available)
// Or via low-level API
```

## Input Sources

### File Path

```rust
.from_file("image.exr")?
.from_file(PathBuf::from("image.exr"))?
.from_file(Path::new("image.exr"))?
```

### Buffered Reader

```rust
use std::io::Cursor;

let bytes: Vec<u8> = load_from_network();
let cursor = Cursor::new(bytes);

.from_buffered(cursor)?
```

### Unbuffered Reader

The library will add buffering:

```rust
use std::fs::File;

let file = File::open("image.exr")?;
.from_unbuffered(file)?
```

## Result Types

The type of `image` depends on your choices:

```rust
// read_all_data_from_file
Image<Layers<AnyChannels<FlatSamples>>>

// read_first_flat_layer_from_file  
Image<Layer<AnyChannels<FlatSamples>>>

// With all_resolution_levels()
Image<Layer<AnyChannels<Levels<FlatSamples>>>>

// With rgba_channels + first_valid_layer
Image<Layer<SpecificChannels<YourStorage, RgbaChannels>>>
```

## Error Handling

```rust
use exr::prelude::*;

fn load_image() -> Result<(), Error> {
    let image = read_all_data_from_file("image.exr")?;
    Ok(())
}

// Or match explicitly
match read_all_data_from_file("image.exr") {
    Ok(image) => { /* use image */ }
    Err(Error::Io(e)) => eprintln!("I/O error: {}", e),
    Err(Error::Invalid(msg)) => eprintln!("Invalid file: {}", msg),
    Err(Error::NotSupported(msg)) => eprintln!("Unsupported: {}", msg),
    Err(e) => eprintln!("Error: {:?}", e),
}
```

## Complete Example

```rust
use exr::prelude::*;

fn main() -> Result<(), Error> {
    // Load render passes
    let image = read()
        .no_deep_data()
        .largest_resolution_level()
        .specific_channels()
            .required("R")
            .required("G")
            .required("B")
            .optional("A", 1.0_f32)
            .optional("Z", f32::INFINITY)
            .collect_pixels(
                |res, _| {
                    vec![(0f32, 0f32, 0f32, 1f32, f32::INFINITY); 
                         res.width() * res.height()]
                },
                |pixels, pos, pixel: (f32, f32, f32, f32, f32)| {
                    pixels[pos.flat_index()] = pixel;
                }
            )
        .first_valid_layer()
        .all_attributes()
        .on_progress(|p| print!("\rLoading: {:.0}%", p * 100.0))
        .from_file("render.exr")?;
    
    println!("\nLoaded {}x{} image", 
        image.layer_data.size.0, 
        image.layer_data.size.1
    );
    
    Ok(())
}
```

## See Also

- [Writing Images](./writing.md) - Create EXR files
- [Deep Data](./deep-data.md) - Variable samples per pixel
- [API Reference: Image](../api-reference/image.md) - Full API documentation
