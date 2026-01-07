# Writing Images

This guide covers creating and writing EXR files with exrs.

## Writing Pipeline Overview

Writing an EXR follows this pattern:

```rust
use exr::prelude::*;

// 1. Create channels
let channels = SpecificChannels::rgba(|pos| (r, g, b, a));

// 2. Create layer(s)
let layer = Layer::new(dimensions, attributes, encoding, channels);

// 3. Create image
let image = Image::from_layer(layer);

// 4. Write
image.write().to_file("output.exr")?;
```

## Convenience Functions

### Simple RGBA

```rust
use exr::prelude::*;

write_rgba_file(
    "output.exr",
    1920, 1080,
    |x, y| {
        let r = compute_red(x, y);
        let g = compute_green(x, y);
        let b = compute_blue(x, y);
        let a = 1.0_f32;
        (r, g, b, a)
    }
)?;
```

### Simple RGB

```rust
use exr::prelude::*;

write_rgb_file(
    "output.exr",
    1920, 1080,
    |x, y| {
        (0.5_f32, 0.3_f32, 0.1_f32)
    }
)?;
```

## Building Images

### From Channels

Simplest way for single-layer images:

```rust
use exr::prelude::*;

let channels = SpecificChannels::rgba(|pos: Vec2<usize>| {
    let x = pos.x() as f32 / 1920.0;
    let y = pos.y() as f32 / 1080.0;
    (x, y, 0.5_f32, 1.0_f32)
});

let image = Image::from_channels((1920, 1080), channels);
image.write().to_file("output.exr")?;
```

### From Layer

More control over layer attributes:

```rust
use exr::prelude::*;

let layer = Layer::new(
    (1920, 1080),
    LayerAttributes::named("beauty"),
    Encoding::FAST_LOSSLESS,
    SpecificChannels::rgba(|_| (0.5, 0.5, 0.5, 1.0))
);

let image = Image::from_layer(layer);
image.write().to_file("output.exr")?;
```

### Multiple Layers

```rust
use exr::prelude::*;

let beauty = Layer::new(
    (1920, 1080),
    LayerAttributes::named("beauty"),
    Encoding::FAST_LOSSLESS,
    SpecificChannels::rgba(|_| (0.5, 0.5, 0.5, 1.0))
);

let depth = Layer::new(
    (1920, 1080),
    LayerAttributes::named("depth"),
    Encoding::FAST_LOSSLESS,
    // Single channel for depth
    SpecificChannels::build()
        .with_channel("Z")
        .with_pixel_fn(|_pos| (1000.0_f32,))
);

let image = Image::empty(ImageAttributes::new(IntegerBounds::from_dimensions((1920, 1080))))
    .with_layer(beauty)
    .with_layer(depth);

image.write().to_file("render.exr")?;
```

## Channel Types

### SpecificChannels (Static)

For known channel layouts:

```rust
use exr::prelude::*;

// RGBA
let channels = SpecificChannels::rgba(|pos| (r, g, b, a));

// RGB
let channels = SpecificChannels::rgb(|pos| (r, g, b));

// Custom channels
let channels = SpecificChannels::build()
    .with_channel("L")
    .with_channel("A")
    .with_channel("B")
    .with_pixel_fn(|pos| (luminance, a_chroma, b_chroma));
```

### AnyChannels (Dynamic)

For runtime-determined channels:

```rust
use exr::prelude::*;

let width = 1920;
let height = 1080;
let pixel_count = width * height;

let channels = AnyChannels::sort(smallvec![
    AnyChannel::new("R", FlatSamples::F32(vec![0.5; pixel_count])),
    AnyChannel::new("G", FlatSamples::F32(vec![0.3; pixel_count])),
    AnyChannel::new("B", FlatSamples::F32(vec![0.1; pixel_count])),
    AnyChannel::new("A", FlatSamples::F16(vec![f16::ONE; pixel_count])),
    AnyChannel::new("Z", FlatSamples::F32(depth_buffer)),
]);

let layer = Layer::new(
    (width, height),
    LayerAttributes::named("main"),
    Encoding::FAST_LOSSLESS,
    channels
);
```

## Sample Types

### Half Precision (f16)

Most common for color channels - good precision, smaller files:

```rust
use exr::prelude::*;

SpecificChannels::rgba(|pos| {
    (
        f16::from_f32(0.5),
        f16::from_f32(0.3),
        f16::from_f32(0.1),
        f16::ONE
    )
});

// Or with FlatSamples
AnyChannel::new("R", FlatSamples::F16(half_values));
```

### Full Precision (f32)

For depth, positions, or when precision matters:

```rust
use exr::prelude::*;

SpecificChannels::build()
    .with_channel("Z")
    .with_pixel_fn(|_| (depth_value as f32,));

// Or with FlatSamples
AnyChannel::new("Z", FlatSamples::F32(depth_values));
```

### Unsigned Integer (u32)

For IDs, object indices, masks:

```rust
use exr::prelude::*;

AnyChannel::new("objectId", FlatSamples::U32(object_ids));
AnyChannel::new("cryptomatte", FlatSamples::U32(crypto_values));
```

## Compression

### Presets

```rust
use exr::prelude::*;

// Fast read/write, larger files
Encoding::UNCOMPRESSED

// Good balance of speed and size
Encoding::FAST_LOSSLESS  // ZIP16

// Best lossless compression (slower)
Encoding::SMALL_LOSSLESS  // PIZ

// Lossy but fast decompression
Encoding::SMALL_LOSSY  // B44
```

### Custom

```rust
use exr::prelude::*;

let encoding = Encoding {
    compression: Compression::ZIP16,  // Or ZIPS, RLE, PIZ, B44, etc.
    blocks: Blocks::ScanLines,
    line_order: LineOrder::Increasing,
};
```

### Compression Comparison

| Compression | Lossless | Speed | Size | Best For |
|-------------|----------|-------|------|----------|
| `Uncompressed` | Yes | Fastest | Largest | Debugging, max speed |
| `RLE` | Yes | Fast | Good | Simple patterns |
| `ZIPS` | Yes | Medium | Good | General, scanline |
| `ZIP16` | Yes | Medium | Good | General, 16-line blocks |
| `PIZ` | Yes | Slow | Smallest | Noisy images |
| `PXR24` | No* | Fast | Small | Float images |
| `B44` | No | Fast | Small | Playback |
| `B44A` | No | Fast | Small | With flat areas |

*PXR24 is lossless for f16 and u32.

## Metadata

### Layer Attributes

```rust
use exr::prelude::*;

let mut attrs = LayerAttributes::named("beauty_pass");

// Optional built-in attributes
attrs.owner = Some(Text::from("Studio Name"));
attrs.comments = Some(Text::from("Final render"));
attrs.software_name = Some(Text::from("My Renderer 2.0"));
attrs.capture_date = Some(/* chrono datetime */);

// Custom attributes
attrs.other.insert(
    Text::from("renderTime"),
    AttributeValue::F32(3600.5)
);
attrs.other.insert(
    Text::from("samples"),  
    AttributeValue::I32(4096)
);
attrs.other.insert(
    Text::from("camera"),
    AttributeValue::Text(Text::from("shot_cam_01"))
);
```

### Image Attributes

```rust
use exr::prelude::*;

let mut img_attrs = ImageAttributes::new(
    IntegerBounds::from_dimensions((1920, 1080))
);

img_attrs.pixel_aspect = 1.0;
img_attrs.time_code = Some(TimeCode { /* ... */ });
```

## Tiles vs Scanlines

### Scanline (Default)

Good for sequential access:

```rust
let encoding = Encoding {
    blocks: Blocks::ScanLines,
    ..Default::default()
};
```

### Tiles

Better for random access and very large images:

```rust
let encoding = Encoding {
    blocks: Blocks::Tiles(TileDescription {
        tile_size: Vec2(64, 64),
        level_mode: LevelMode::Singular,
        rounding_mode: RoundingMode::Down,
    }),
    ..Default::default()
};
```

## Mip Maps

```rust
use exr::prelude::*;

// Generate mip levels
let level0: Vec<f32> = original_pixels;
let level1: Vec<f32> = downsample(&level0, 2);
let level2: Vec<f32> = downsample(&level1, 2);

let levels = Levels::Mip {
    rounding_mode: RoundingMode::Down,
    level_data: vec![
        FlatSamples::F32(level0),
        FlatSamples::F32(level1),
        FlatSamples::F32(level2),
    ].into()
};

let channel = AnyChannel::new("R", levels);
```

## Writing Options

### Progress Callback

```rust
image.write()
    .on_progress(|progress: f64| {
        print!("\rWriting: {:.1}%", progress * 100.0);
    })
    .to_file("output.exr")?;
```

### Non-Parallel

Disable multi-threaded compression:

```rust
image.write()
    .non_parallel()
    .to_file("output.exr")?;
```

## Output Destinations

### File Path

```rust
image.write().to_file("output.exr")?;
image.write().to_file(Path::new("output.exr"))?;
image.write().to_file(PathBuf::from("output.exr"))?;
```

### Buffered Writer

```rust
use std::io::Cursor;

let mut buffer = Vec::new();
let cursor = Cursor::new(&mut buffer);
image.write().to_buffered(cursor)?;
// buffer now contains EXR bytes
```

### Unbuffered Writer

```rust
use std::fs::File;

let file = File::create("output.exr")?;
image.write().to_unbuffered(file)?;
```

## Complete Examples

### Render Output

```rust
use exr::prelude::*;

fn save_render(
    path: &str,
    width: usize,
    height: usize,
    rgba: &[(f32, f32, f32, f32)],
    depth: &[f32],
) -> Result<(), Error> {
    let beauty = Layer::new(
        (width, height),
        LayerAttributes::named("rgba"),
        Encoding::FAST_LOSSLESS,
        SpecificChannels::rgba(|pos| rgba[pos.y() * width + pos.x()])
    );
    
    let depth_layer = Layer::new(
        (width, height),
        LayerAttributes::named("depth"),
        Encoding {
            compression: Compression::ZIPS,
            ..Default::default()
        },
        SpecificChannels::build()
            .with_channel("Z")
            .with_pixel_fn(|pos| (depth[pos.y() * width + pos.x()],))
    );
    
    Image::empty(ImageAttributes::new(IntegerBounds::from_dimensions((width, height))))
        .with_layer(beauty)
        .with_layer(depth_layer)
        .write()
        .on_progress(|p| print!("\rSaving: {:.0}%", p * 100.0))
        .to_file(path)?;
    
    println!(" Done!");
    Ok(())
}
```

### Dynamic Channel Creation

```rust
use exr::prelude::*;

fn save_aov_bundle(
    path: &str,
    width: usize,
    height: usize,
    aovs: &[(&str, Vec<f32>)],
) -> Result<(), Error> {
    let pixel_count = width * height;
    
    let channels: SmallVec<[AnyChannel<FlatSamples>; 8]> = aovs.iter()
        .map(|(name, data)| {
            AnyChannel::new(*name, FlatSamples::F32(data.clone()))
        })
        .collect();
    
    let layer = Layer::new(
        (width, height),
        LayerAttributes::named("aovs"),
        Encoding::FAST_LOSSLESS,
        AnyChannels::sort(channels)
    );
    
    Image::from_layer(layer)
        .write()
        .to_file(path)
}
```

## See Also

- [Reading Images](./reading.md) - Load EXR files
- [Deep Data](./deep-data.md) - Variable samples per pixel
- [API Reference: Image](../api-reference/image.md) - Full API documentation
