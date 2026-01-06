# Deep Data Support in exrs

OpenEXR 2.0 deep data implementation for the exrs library.

## Overview

Deep images store **variable samples per pixel** - from 0 to thousands. This enables:
- Volumetric rendering (smoke, fog, hair)
- Particle systems with depth
- Multi-layer compositing without pre-flattening

```
Flat image:    1 sample/pixel  →  RGBA per pixel
Deep image:    N samples/pixel →  N × RGBA per pixel, each at different depth
```

## Architecture

### Module Structure

```
src/
├── image/
│   ├── deep.rs              # DeepSamples, DeepChannelData types
│   ├── read/deep.rs         # High-level read API
│   └── write/deep.rs        # High-level write API
├── block/
│   └── deep.rs              # Block compression/decompression, parallel processing
└── compression/
    └── deep.rs              # Sample table & data compression algorithms
```

### Data Flow

```
                    READING                                    WRITING
                    
File on disk                                      DeepSamples in memory
     │                                                   │
     ▼                                                   ▼
CompressedDeepScanLineBlock                      pack_deep_channels()
     │                                                   │
     ├── compressed_pixel_offset_table           compress_sample_table()
     │        │                                          │
     │        ▼                                          ▼
     │   decompress_sample_table()               compressed_pixel_offset_table
     │        │                                          │
     │        ▼                                          │
     │   cumulative i32[] → sample_offsets u32[]         │
     │                                                   │
     └── compressed_sample_data_le               compress_sample_data()
              │                                          │
              ▼                                          ▼
         decompress_sample_data()                compressed_sample_data_le
              │                                          │
              ▼                                          ▼
         unpack_deep_channels()                  CompressedDeepScanLineBlock
              │                                          │
              ▼                                          ▼
         DeepSamples                             File on disk
```

## Core Types

### DeepSamples

Central data structure for deep pixel data:

```rust
pub struct DeepSamples {
    pub sample_offsets: Vec<u32>,        // Cumulative counts, length = width × height
    pub channels: Vec<DeepChannelData>,  // Channel arrays, each length = total_samples
    pub width: usize,
    pub height: usize,
}
```

**Sample offset encoding** - cumulative counts with implicit leading zero:

```
Pixel:          [0]  [1]  [2]  [3]
Sample count:    2    0    3    1
Offsets:         2    2    5    6   ← sample_offsets[]
                 ↑    ↑    ↑    ↑
              0..2  2..2 2..5  5..6  ← sample ranges
```

To get samples for pixel N: `range = offsets[N-1]..offsets[N]` (with `offsets[-1] = 0`).

### DeepChannelData

Type-safe channel storage:

```rust
pub enum DeepChannelData {
    F16(Vec<f16>),  // 16-bit float (common for RGBA)
    F32(Vec<f32>),  // 32-bit float (depth, alpha)
    U32(Vec<u32>),  // 32-bit uint (object IDs)
}
```

All channel arrays have identical length = `total_samples`.

## File Format

### Block Structure

OpenEXR deep blocks contain two compressed sections:

```
CompressedDeepScanLineBlock
├── y_coordinate: i32
├── compressed_pixel_offset_table: Vec<i8>    ← ZIP/RLE compressed
├── compressed_sample_data_le: Vec<u8>        ← ZIP/RLE compressed
└── decompressed_sample_data_size: usize      ← for validation
```

### Offset Table Format

**Critical difference from in-memory format:**

| Property | File Format | In-Memory (DeepSamples) |
|----------|-------------|-------------------------|
| Scope | Per-scanline | Full image |
| Reset | Restarts at 0 each line | Continuous |
| Type | i32 | u32 |

Example for 3-pixel-wide, 2-line block:
```
Per-pixel counts:   [2, 1, 3]  [0, 2, 1]
File offset table:  [2, 3, 6,   0, 2, 3]  ← restarts each line!
Memory offsets:     [2, 3, 6,   6, 8, 9]  ← continuous
```

### Sample Data Layout

Interleaved by pixel, then by channel (structure-of-arrays within each pixel):

```
Pixel 0 (2 samples): [R0, R1], [G0, G1], [B0, B1], [A0, A1]
Pixel 1 (1 sample):  [R0], [G0], [B0], [A0]
Pixel 2 (3 samples): [R0, R1, R2], [G0, G1, G2], ...
```

All values little-endian.

## Parallel Decompression

### Design Decision

Four approaches were considered:

| Approach | Breaking Changes | Complexity | Code Reuse |
|----------|------------------|------------|------------|
| A. Extend `UncompressedBlock` | High | Medium | Maximum |
| **B. Separate `ParallelDeepBlockDecompressor`** | **None** | **Medium** | **Moderate** |
| C. Generic `ParallelDecompressor<T>` trait | Low | High | Maximum |
| D. Simple `par_iter` at high level | None | Low | Minimal |

**Chosen: Option B** - mirrors existing `ParallelBlockDecompressor` pattern without API changes.

### Implementation

```rust
#[cfg(feature = "rayon")]
pub struct ParallelDeepBlockDecompressor<R: ChunksReader> {
    remaining_chunks: R,
    sender: mpsc::Sender<Result<DeepUncompressedBlock>>,
    receiver: mpsc::Receiver<Result<DeepUncompressedBlock>>,
    currently_decompressing_count: usize,
    max_threads: usize,
    shared_meta_data_ref: Arc<MetaData>,
    pool: rayon_core::ThreadPool,
}
```

**Pipeline:**
1. Main thread reads compressed chunks from file
2. Chunks dispatched to thread pool for decompression
3. Decompressed blocks sent via channel
4. Iterator yields blocks as they complete (out-of-order OK)
5. Blocks sorted by y-coordinate before merging

### Performance

Benchmark results (release build, ZIP-compressed files):

| File | Samples | Sequential | Parallel | Speedup |
|------|---------|------------|----------|---------|
| Teaset720p.exr | 997K | 199ms | 79ms | **2.52x** |
| Ground.exr | 360K | 84ms | 37ms | **2.27x** |
| MiniCooper720p.exr | 932K | 150ms | 84ms | **1.79x** |
| Balls.exr | 94K | 35ms | 23ms | **1.52x** |

Speedup varies with compression ratio and I/O overhead.

## Block Merging

Deep scanline files contain multiple blocks (1-32 lines each). `merge_deep_blocks()` combines them:

```rust
fn merge_deep_blocks(
    blocks: Vec<(usize, DeepSamples)>,  // (y_offset, samples)
    total_width: usize,
    total_height: usize,
) -> Result<DeepSamples>
```

**Algorithm:**

1. **Build offset table** - `combined_offsets[total_pixels + 1]` with leading 0
2. **Collect counts** - copy per-pixel counts from each block to correct image positions
3. **Prefix sum** - convert individual counts to cumulative offsets
4. **Allocate channels** - create output arrays sized to `total_samples`
5. **Copy data** - place each block's samples at correct positions

**Why leading zero?** Simplifies the copy loop - for pixel N, destination starts at `combined_offsets[N]`. Final `sample_offsets` is `combined_offsets[1..]`.

## Compression

### Supported Methods

Deep data supports all standard OpenEXR compressions:

| Method | Sample Table | Sample Data | Notes |
|--------|--------------|-------------|-------|
| Uncompressed | Raw i32[] | Raw bytes | Baseline |
| RLE | RLE on i32 | RLE on bytes | Good for sparse data |
| ZIP | Deflate | Deflate | Most common in production |
| ZIPS | Deflate | Deflate | Single-scanline ZIP |

**Not supported for deep:** PIZ, PXR24, B44, DWAA/DWAB (lossy methods incompatible with variable-length data).

### Sample Table Compression

```rust
pub fn compress_sample_table(
    compression: Compression,
    cumulative_counts: &[i32],
) -> Result<Vec<u8>>

pub fn decompress_sample_table(
    compression: Compression,
    compressed: &[u8],
    width: usize,
    height: usize,
) -> Result<Vec<i32>>
```

Table is validated after decompression: must be monotonically non-decreasing.

## API Usage

### Reading

```rust
use exr::image::read::deep::read_first_deep_layer_from_file;

let image = read_first_deep_layer_from_file("particles.exr")?;
let samples = &image.layer_data.channel_data.list[0].sample_data;

println!("Total samples: {}", samples.total_samples());
println!("Max per pixel: {}", samples.max_samples_per_pixel());

// Access specific pixel
for (x, y, count) in samples.pixels_with_counts() {
    if count > 0 {
        let (start, end) = samples.sample_range(y * width + x);
        // Process samples[start..end] for each channel
    }
}
```

Builder API with options:

```rust
use exr::image::read::deep::read_deep;

let image = read_deep()
    .all_channels()
    .first_valid_layer()
    .all_attributes()
    .non_parallel()      // Force sequential (for debugging)
    .pedantic()          // Strict validation
    .from_file("deep.exr")?;
```

### Writing

```rust
use exr::image::write::deep::write_deep_file;

let mut samples = DeepSamples::new(width, height);
samples.set_cumulative_counts(offsets)?;
samples.allocate_channels(&channel_list);

// Fill channel data...

write_deep_file(
    "output.exr",
    samples,
    &channel_list,
    Compression::ZIP,
)?;
```

## Validation

`DeepSamples::validate()` checks:

1. **Offset array length** = width × height
2. **Monotonicity** - offsets never decrease
3. **Channel lengths** - all channels have `total_samples` elements
4. **No overflow** - total samples fits in memory

## Limitations

1. **Single-layer multi-file** - `all_layers()` mode re-reads file for each layer (TODO: optimize)
2. **Tile support** - implemented but less tested than scanlines
3. **Mipmap/ripmap** - not implemented for deep tiles
4. **Memory** - full image loaded to memory (no streaming for deep merge step)

## Testing

```bash
# All deep tests
cargo test deep

# Parallel vs sequential benchmark
cargo test --release benchmark_deep_read -- --nocapture

# Roundtrip validation
cargo test roundtrip_deep
```

Test files location: `tests/images/valid/openexr/v2/`

## References

- [OpenEXR Deep Data Documentation](https://openexr.com/en/latest/TechnicalIntroduction.html)
- [OpenEXR File Layout](https://openexr.com/en/latest/OpenEXRFileLayout.html)
- [Deep Compositing Paper (Hillman et al.)](https://graphics.pixar.com/library/DeepCompositing/)
