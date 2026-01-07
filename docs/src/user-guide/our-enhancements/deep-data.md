# Deep Data Support

Our implementation brings full OpenEXR 2.0 deep data support to the exrs library.

## What We Added

### Core Types (`src/image/deep.rs`)

New module providing deep data structures:

```rust
/// Variable samples per pixel
pub struct DeepSamples {
    pub sample_offsets: Vec<u32>,      // Cumulative counts
    pub channels: Vec<DeepChannelData>, // Channel arrays
    pub width: usize,
    pub height: usize,
}

pub enum DeepChannelData {
    F16(Vec<f16>),
    F32(Vec<f32>),
    U32(Vec<u32>),
}
```

**Features:**
- `total_samples()` - Total sample count across all pixels
- `max_samples_per_pixel()` - Maximum density
- `sample_count(x, y)` - Count at specific pixel
- `sample_range(idx)` - Array range for pixel
- Memory-efficient cumulative offset storage

### Block-Level Support (`src/block/deep.rs`)

843 lines of deep block processing:

```rust
pub struct DeepBlockReader { ... }
pub struct DeepBlockWriter { ... }

// Decompression
pub fn decompress_deep_scanline_block(...) -> Result<DeepUncompressedBlock>
pub fn decompress_deep_tile_block(...) -> Result<DeepUncompressedBlock>

// Compression  
pub fn compress_deep_block(...) -> Result<CompressedDeepBlock>
```

**Capabilities:**
- Sample count table encoding/decoding
- Deep scanline and tile support
- Integration with all supported compression methods
- Validation of deep block structure

### High-Level Reading (`src/image/read/deep.rs`)

919 lines providing intuitive API:

```rust
use exr::image::read::deep::{read_deep, read_first_deep_layer_from_file};

// Simple
let image = read_first_deep_layer_from_file("particles.exr")?;

// Builder pattern
let image = read_deep()
    .all_channels()
    .first_valid_layer()  // or .all_layers()
    .all_attributes()
    .non_parallel()       // optional
    .pedantic()           // optional
    .on_progress(|p| ...) // optional
    .from_file("volumetric.exr")?;
```

### High-Level Writing (`src/image/write/deep.rs`)

636 lines for deep output:

```rust
use exr::image::write::deep::write_deep_image_to_file;
use exr::compression::Compression;

write_deep_image_to_file(
    "output.exr",
    &deep_image,
    Compression::ZIP1,
)?;
```

**Features:**
- Automatic sample table generation
- All lossless compression methods
- Scanline and tile modes
- Multi-layer deep images

### Unified Reader (`src/image/read/any_samples.rs`)

764 lines for format-agnostic reading:

```rust
use exr::prelude::*;

let image = read_first_any_layer_from_file("unknown.exr")?;

match &channel.sample_data {
    DeepAndFlatSamples::Deep(samples) => { /* deep */ }
    DeepAndFlatSamples::Flat(samples) => { /* flat */ }
}
```

### Parallel Decompression

Multi-threaded deep block processing:

```rust
#[cfg(feature = "rayon")]
pub struct ParallelDeepBlockDecompressor<R: ChunksReader> {
    remaining_chunks: R,
    sender: mpsc::Sender<Result<DeepUncompressedBlock>>,
    receiver: mpsc::Receiver<Result<DeepUncompressedBlock>>,
    pool: rayon_core::ThreadPool,
    // ...
}
```

**Pipeline:**
1. Main thread reads compressed chunks
2. Chunks dispatched to thread pool
3. Decompressed blocks sent via channel
4. Blocks sorted by y-coordinate
5. Merged into final `DeepSamples`

## Data Flow

```
                    READING                          WRITING
                    
File on disk                                 DeepSamples in memory
     │                                              │
     ▼                                              ▼
CompressedDeepScanLineBlock                  pack_deep_channels()
     │                                              │
     ├── sample_count_table ◄───────────────► compress_sample_table()
     │         │                                    │
     │         ▼                                    │
     │   decompress & validate                      │
     │         │                                    │
     │         ▼                                    │
     │   cumulative offsets                         │
     │                                              │
     └── sample_data ◄──────────────────────► compress_sample_data()
              │                                     │
              ▼                                     ▼
         decompress                          CompressedDeepBlock
              │                                     │
              ▼                                     ▼
         unpack_channels                     File on disk
              │
              ▼
         DeepSamples
```

## Compression Support

| Method | Deep Support | Notes |
|--------|--------------|-------|
| Uncompressed | Yes | Fastest I/O |
| RLE | Yes | Good for sparse |
| ZIPS | Yes | Single scanline |
| ZIP | Yes | **Recommended** |
| PIZ | No | Incompatible |
| PXR24 | No | Float truncation |
| B44/B44A | No | Fixed blocks |

## File Format Details

### Block Structure

```
CompressedDeepScanLineBlock
├── y_coordinate: i32
├── compressed_pixel_offset_table: Vec<u8>  ← ZIP compressed
├── compressed_sample_data: Vec<u8>         ← ZIP compressed
└── decompressed_sample_data_size: usize
```

### Offset Table Format

**Critical difference from memory format:**

| Property | File | Memory |
|----------|------|--------|
| Scope | Per-scanline | Full image |
| Reset | Each line → 0 | Continuous |
| Type | i32 | u32 |

```
Per-line counts: [2, 1, 3]  [0, 2, 1]
File offsets:    [2, 3, 6,   0, 2, 3]  ← restarts!
Memory offsets:  [2, 3, 6,   6, 8, 9]  ← continuous
```

## Tests Added

### Unit Tests

- `deep_read.rs` - 176 lines
- `deep_benchmark.rs` - 61 lines
- Deep roundtrip tests
- Deep fuzzing integration

### Test Images

Official OpenEXR deep test files:
- `MiniCooper720p.exr` (932K samples)
- `PiranhnaAlienRun720p.exr`
- `Teaset720p.exr` (997K samples)
- `Ground.exr`, `Balls.exr`, etc.

## Performance

Benchmark results (release build, ZIP compression):

| File | Samples | Sequential | Parallel | Speedup |
|------|---------|------------|----------|---------|
| Teaset720p.exr | 997K | 199ms | 79ms | 2.52x |
| Ground.exr | 360K | 84ms | 37ms | 2.27x |
| MiniCooper720p.exr | 932K | 150ms | 84ms | 1.79x |
| Balls.exr | 94K | 35ms | 23ms | 1.52x |

## Known Limitations

1. **Memory** - Full image loaded (no streaming for merge step)
2. **Tiles** - Implemented but less tested than scanlines
3. **Mipmap** - Not implemented for deep tiles
4. **Multi-layer** - Re-reads file for each layer (optimization pending)

## Example Usage

### Reading and Processing

```rust
use exr::image::read::deep::read_first_deep_layer_from_file;
use exr::image::deep::DeepChannelData;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let image = read_first_deep_layer_from_file("particles.exr")?;
    let samples = &image.layer_data.channel_data.list[0].sample_data;
    
    println!("Image: {}x{}", samples.width, samples.height);
    println!("Total samples: {}", samples.total_samples());
    
    // Find densest pixel
    let mut max_count = 0;
    let mut max_pos = (0, 0);
    
    for y in 0..samples.height {
        for x in 0..samples.width {
            let count = samples.sample_count(x, y);
            if count > max_count {
                max_count = count;
                max_pos = (x, y);
            }
        }
    }
    
    println!("Densest pixel: {:?} with {} samples", max_pos, max_count);
    Ok(())
}
```

### Flattening Deep to Flat

```rust
fn flatten_deep(samples: &DeepSamples, channel_idx: usize) -> Vec<f32> {
    let mut result = vec![0.0; samples.width * samples.height];
    
    for y in 0..samples.height {
        for x in 0..samples.width {
            let pixel_idx = y * samples.width + x;
            let (start, end) = samples.sample_range(pixel_idx);
            
            if let DeepChannelData::F32(data) = &samples.channels[channel_idx] {
                // Simple over composite (front-to-back)
                let mut accumulated = 0.0;
                for i in start..end {
                    accumulated += data[i];  // Simplified - real composite uses alpha
                }
                result[pixel_idx] = accumulated;
            }
        }
    }
    
    result
}
```

## See Also

- [User Guide: Deep Data](../deep-data.md) - User-facing documentation
- [API Reference: Deep](../../api-reference/deep.md) - Full API docs
- [Developer Guide: Data Flow](../../developer-guide/dataflow.md) - Internal architecture
