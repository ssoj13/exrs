# Deep Data

Deep data (OpenEXR 2.0+) stores a **variable number of samples per pixel**. This is essential for volumetric effects, particles, and advanced compositing.

## Flat vs Deep

```
Flat image:  1 sample per pixel
  Pixel (10, 20) = RGBA(0.5, 0.3, 0.1, 1.0)

Deep image:  N samples per pixel (0 to thousands)
  Pixel (10, 20) = [
    {Z=1.5, RGBA(0.2, 0.1, 0.0, 0.3)},  // Smoke layer
    {Z=5.0, RGBA(0.8, 0.6, 0.4, 1.0)},  // Solid object
    {Z=8.2, RGBA(0.1, 0.1, 0.1, 0.1)},  // Background fog
  ]
```

## When to Use Deep Data

| Use Case | Why Deep? |
|----------|-----------|
| **Volumetrics** | Smoke, fog, clouds with proper depth sorting |
| **Particles** | Each particle at its true depth |
| **Hair/Fur** | Multiple transparent strands per pixel |
| **Deep Compositing** | Layer elements without pre-flattening |
| **Hold-out Mattes** | Correct occlusion with complex geometry |

## Reading Deep Images

### Simple API

```rust
use exr::image::read::deep::read_first_deep_layer_from_file;

fn main() {
    let image = read_first_deep_layer_from_file("particles.exr")
        .expect("Failed to read deep EXR");
    
    let layer = &image.layer_data;
    let samples = &layer.channel_data.list[0].sample_data;
    
    println!("Image size: {}x{}", samples.width, samples.height);
    println!("Total samples: {}", samples.total_samples());
    println!("Max per pixel: {}", samples.max_samples_per_pixel());
}
```

### Builder API

```rust
use exr::image::read::deep::read_deep;

let image = read_deep()
    .all_channels()
    .first_valid_layer()
    .all_attributes()
    .from_file("volumetric.exr")?;

// Multiple layers
let multi = read_deep()
    .all_channels()
    .all_layers()
    .all_attributes()
    .from_file("layered_deep.exr")?;
```

### Options

```rust
use exr::image::read::deep::read_deep;

let image = read_deep()
    .all_channels()
    .first_valid_layer()
    .all_attributes()
    .non_parallel()     // Single-threaded (debugging)
    .pedantic()         // Strict validation
    .on_progress(|p| println!("{}%", (p * 100.0) as i32))
    .from_file("deep.exr")?;
```

## The DeepSamples Structure

Deep data uses a Struct-of-Arrays (SoA) layout for efficiency:

```rust
pub struct DeepSamples {
    /// Cumulative sample counts per pixel
    /// Length = width * height
    pub sample_offsets: Vec<u32>,
    
    /// Channel data arrays, each length = total_samples
    pub channels: Vec<DeepChannelData>,
    
    pub width: usize,
    pub height: usize,
}

pub enum DeepChannelData {
    F16(Vec<f16>),
    F32(Vec<f32>),
    U32(Vec<u32>),
}
```

### Sample Offset Encoding

Offsets are cumulative (prefix sum):

```
Pixel:          [0]  [1]  [2]  [3]
Sample count:    2    0    3    1
Offsets:         2    2    5    6
                 ^    ^    ^    ^
Ranges:       0..2  2..2 2..5  5..6
```

### Accessing Samples

```rust
use exr::image::deep::{DeepSamples, DeepChannelData};

fn process_samples(samples: &DeepSamples) {
    // Get count for a specific pixel
    let count = samples.sample_count(10, 20);
    println!("Pixel (10, 20) has {} samples", count);
    
    // Get sample range for direct array access
    let pixel_idx = 20 * samples.width + 10;
    let (start, end) = samples.sample_range(pixel_idx);
    
    // Access channel data
    for (ch_idx, channel) in samples.channels.iter().enumerate() {
        match channel {
            DeepChannelData::F16(data) => {
                for i in start..end {
                    let value = data[i];
                    println!("  Ch{} sample {}: {}", ch_idx, i - start, value);
                }
            }
            DeepChannelData::F32(data) => {
                for i in start..end {
                    let depth = data[i];
                    println!("  Ch{} sample {}: {}", ch_idx, i - start, depth);
                }
            }
            DeepChannelData::U32(data) => {
                for i in start..end {
                    let id = data[i];
                    println!("  Ch{} sample {}: {}", ch_idx, i - start, id);
                }
            }
        }
    }
}
```

### Iterating All Pixels

```rust
fn iterate_deep_image(samples: &DeepSamples) {
    for y in 0..samples.height {
        for x in 0..samples.width {
            let count = samples.sample_count(x, y);
            if count == 0 {
                continue;  // Skip empty pixels
            }
            
            let pixel_idx = y * samples.width + x;
            let (start, end) = samples.sample_range(pixel_idx);
            
            // Process samples at this pixel
            for sample_idx in start..end {
                // Access all channels for this sample
            }
        }
    }
}
```

## Writing Deep Images

```rust
use exr::image::read::deep::read_first_deep_layer_from_file;
use exr::image::write::deep::write_deep_image_to_file;
use exr::compression::Compression;

fn main() {
    // Read a deep image
    let image = read_first_deep_layer_from_file("input.exr").unwrap();
    
    // Write with compression
    write_deep_image_to_file(
        "output.exr",
        &image,
        Compression::ZIP1,  // Recommended for deep
    ).unwrap();
}
```

## Compression for Deep Data

Not all compression methods support deep data:

| Method | Deep Support | Notes |
|--------|--------------|-------|
| `Uncompressed` | Yes | Fastest, largest |
| `RLE` | Yes | Good for sparse data |
| `ZIPS` | Yes | Single scanline ZIP |
| `ZIP` | Yes | **Recommended** |
| `PIZ` | No | Lossy elements incompatible |
| `PXR24` | No | Float truncation |
| `B44/B44A` | No | Fixed-size blocks |
| `DWAA/DWAB` | No | Not implemented |

## Parallel Decompression

Deep reading supports parallel decompression:

```
Sequential:  File → Read → Decompress → Decompress → ... → Merge
Parallel:    File → Read ─┬→ Thread 1 → ┐
                          ├→ Thread 2 → ├→ Sort → Merge
                          └→ Thread 3 → ┘
```

Benchmark results (release build):

| File | Samples | Sequential | Parallel | Speedup |
|------|---------|------------|----------|---------|
| Teaset720p.exr | 997K | 199ms | 79ms | **2.52x** |
| Ground.exr | 360K | 84ms | 37ms | **2.27x** |
| MiniCooper720p.exr | 932K | 150ms | 84ms | **1.79x** |

## Unified Deep/Flat Reading

When you don't know if a file is deep or flat:

```rust
use exr::prelude::*;

let image = read_first_any_layer_from_file("unknown.exr")?;

// Check what we got
for channel in &image.layer_data.channel_data.list {
    match &channel.sample_data {
        DeepAndFlatSamples::Deep(deep) => {
            println!("{}: Deep, {} total samples", 
                channel.name, deep.total_samples());
        }
        DeepAndFlatSamples::Flat(flat) => {
            println!("{}: Flat, {} samples", 
                channel.name, flat.len());
        }
    }
}
```

## Deep Compositing Workflow

Deep compositing preserves depth information through the pipeline:

```
                    Traditional                    Deep
                    
Render A       ─┐                             ─┐
                ├─> Pre-multiply ─> Composite   ├─> Deep Merge ─> Flatten
Render B       ─┘                             ─┘
                    
                    ⚠ Artifacts at edges        ✓ Correct everywhere
```

### Basic Deep Composite

```rust
fn deep_over(front: &DeepSamples, back: &DeepSamples) -> DeepSamples {
    // For each pixel:
    // 1. Combine samples from both images
    // 2. Sort by depth (Z)
    // 3. Composite front-to-back
    // ... implementation depends on your needs
}
```

## Validation

`DeepSamples` validates its structure:

```rust
impl DeepSamples {
    pub fn validate(&self) -> Result<()> {
        // 1. Offset array length = width * height
        // 2. Offsets never decrease (monotonic)
        // 3. All channels have total_samples elements
        // 4. No overflow
    }
}
```

## Memory Considerations

Deep images can be large:

```
Flat 1920x1080 RGBA f16:  1920 * 1080 * 4 * 2 = 16 MB
Deep same resolution:     Depends on content
  - Sparse (avg 0.1):     ~1.6 MB
  - Dense (avg 10):       ~160 MB
  - Very dense (avg 100): ~1.6 GB
```

Tips:
- Check `total_samples()` before processing
- Use `sample_count()` to skip empty pixels
- Process in streaming fashion when possible

## Limitations

Current implementation limitations:

1. **Full image loading** - Entire deep image loaded to memory
2. **Single-layer optimization** - Multi-layer re-reads file
3. **Tile support** - Implemented but less tested
4. **No mipmap** - Mip/rip maps not supported for deep

## See Also

- [Our Deep Data Enhancements](./our-enhancements/deep-data.md) - What we added
- [API Reference: Deep](../api-reference/deep.md) - Full API documentation
- [OpenEXR Deep Compositing](https://openexr.com/en/latest/TechnicalIntroduction.html) - Official docs
