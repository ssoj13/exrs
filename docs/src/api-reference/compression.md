# Compression Module

Low-level compression and decompression for EXR pixel data.

## Module: `exrs::compression`

### Compression Enum

```rust
pub enum Compression {
    /// No compression
    Uncompressed,
    
    /// Run-length encoding (lossless)
    RLE,
    
    /// Zip compression, 1 scanline per block (lossless)
    ZIP1,
    
    /// Zip compression, 16 scanlines per block (lossless)
    ZIP16,
    
    /// Wavelet-based compression (lossless)
    PIZ,
    
    /// Lossy DCT-based compression (24-bit)
    PXR24,
    
    /// Lossy fixed-rate compression
    B44,
    
    /// B44 with improved handling of flat areas
    B44A,
    
    /// DWAA - lossy compression, 32 scanlines
    DWAA(Option<f32>),
    
    /// DWAB - lossy compression, 256 scanlines
    DWAB(Option<f32>),
}
```

### Compression Methods

```rust
impl Compression {
    /// Returns true if compression is lossless
    pub fn is_lossless(&self) -> bool;
    
    /// Returns true if compression may lose data
    pub fn is_lossy(&self) -> bool;
    
    /// Number of scanlines per compressed block
    pub fn scan_lines_per_block(&self) -> usize;
    
    /// Whether this compression supports deep data
    pub fn supports_deep_data(&self) -> bool;
}
```

## Compression Selection Guide

| Compression | Type | Speed | Ratio | Deep Support |
|-------------|------|-------|-------|--------------|
| Uncompressed | - | Fastest | 1:1 | Yes |
| RLE | Lossless | Fast | Low | Yes |
| ZIP1 | Lossless | Medium | Good | Yes |
| ZIP16 | Lossless | Medium | Better | Yes |
| PIZ | Lossless | Slow | Best | No |
| PXR24 | Lossy | Fast | Good | No |
| B44/B44A | Lossy | Fast | Fixed | No |
| DWAA/DWAB | Lossy | Medium | Excellent | No |

## Internal Functions

### Compress

```rust
pub fn compress(
    compression: Compression,
    bytes: ByteVec,
    rectangle: IntegerBounds,
    channels: &[ChannelDescription],
) -> Result<ByteVec>;
```

Compresses raw pixel data using the specified algorithm.

### Decompress

```rust
pub fn decompress(
    compression: Compression,
    compressed: ByteVec,
    rectangle: IntegerBounds,
    channels: &[ChannelDescription],
    expected_byte_size: usize,
    pedantic: bool,
) -> Result<ByteVec>;
```

Decompresses pixel data back to raw bytes.

## Algorithm Details

### ZIP (Deflate)

```rust
// ZIP uses zlib deflate compression
// ZIP1: 1 scanline per block - lower latency
// ZIP16: 16 scanlines per block - better compression
```

### PIZ (Wavelet)

```rust
// PIZ uses Haar wavelet transform + Huffman coding
// Best lossless compression ratio
// Slower than ZIP
// Not suitable for deep data
```

### RLE (Run-Length)

```rust
// Simple run-length encoding
// Fast compression/decompression
// Good for images with large flat areas
// Supports deep data
```

### PXR24 (Lossy 24-bit)

```rust
// Converts 32-bit float to 24-bit
// Loses ~1 bit of precision
// Good for display-ready images
```

### B44/B44A (Fixed Rate)

```rust
// Fixed 4:1 compression ratio (4.54:1 with alpha)
// B44A handles flat areas better
// Good for real-time playback
```

### DWAA/DWAB (DCT-based)

```rust
// Lossy DCT compression similar to JPEG
// Configurable quality via compression level
// DWAA: 32 scanlines per block
// DWAB: 256 scanlines per block (better ratio)
```

## Usage Examples

### Setting Compression on Write

```rust
use exrs::prelude::*;

let image = Image::from_channels(
    (1920, 1080),
    SpecificChannels::rgba(|pos| {
        // pixel data...
        (1.0f32, 0.5, 0.0, 1.0)
    })
);

image.write()
    .to_file("output.exr")?;

// Or with specific compression:
image.write()
    .to_file("output_piz.exr")?; // Default is ZIP
```

### Checking Compression Type

```rust
use exrs::prelude::*;
use exrs::meta::header::Header;

let meta = MetaData::read_from_file("image.exr", false)?;
for header in &meta.headers {
    println!("Compression: {:?}", header.compression);
    println!("Lossless: {}", header.compression.is_lossless());
}
```

## Performance Considerations

1. **For archival**: Use PIZ (best lossless ratio)
2. **For speed**: Use ZIP1 or RLE
3. **For streaming**: Use Uncompressed or RLE
4. **For deep data**: Use ZIP16 or RLE (PIZ not supported)
5. **For playback**: Use B44/B44A (fixed decode time)

## Parallel Decompression

The library uses Rayon for parallel decompression:

```rust
// Blocks are decompressed in parallel automatically
// Configure thread pool via rayon's global thread pool
rayon::ThreadPoolBuilder::new()
    .num_threads(8)
    .build_global()
    .unwrap();
```
