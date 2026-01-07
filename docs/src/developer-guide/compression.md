# Compression Algorithms

exrs implements most OpenEXR compression methods. This document describes each algorithm.

## Overview

| Method | Code | Lossless | Deep | Block Size |
|--------|------|----------|------|------------|
| Uncompressed | 0 | Yes | Yes | 1 line |
| RLE | 1 | Yes | Yes | 1 line |
| ZIPS | 2 | Yes | Yes | 1 line |
| ZIP | 3 | Yes | Yes | 16 lines |
| PIZ | 4 | Yes | No | 32 lines |
| PXR24 | 5 | No* | No | 16 lines |
| B44 | 6 | No | No | 32 lines |
| B44A | 7 | No | No | 32 lines |
| DWAA | 8 | No | No | 32 lines |
| DWAB | 9 | No | No | 256 lines |

*PXR24 is lossless for f16 and u32, lossy for f32.

## Pre/Post Processing

All compression methods share common pre/post processing:

### Pre-processing (before compression)

```rust
// 1. Convert to little-endian
for sample in samples {
    sample.to_le_bytes();
}

// 2. Separate bytes for better compression
// [A1A2 B1B2 C1C2] → [A1B1C1 A2B2C2]
separate_bytes_fragments(&mut bytes);

// 3. Delta encoding
// [10, 13, 16] → [10, 3, 3]
samples_to_differences(&mut bytes);
```

### Post-processing (after decompression)

```rust
// Reverse order
differences_to_samples(&mut bytes);  // [10, 3, 3] → [10, 13, 16]
interleave_byte_blocks(&mut bytes);  // [A1B1C1 A2B2C2] → [A1A2 B1B2 C1C2]
to_native_endian(&mut bytes);
```

## Uncompressed

Simply stores raw bytes in little-endian format.

**Pros:**
- Fastest read/write
- No CPU overhead

**Cons:**
- Largest file size

**Use when:**
- Maximum I/O speed needed
- Debugging
- SSD/NVMe storage

## RLE (Run-Length Encoding)

Compresses runs of identical bytes.

```rust
pub fn compress(uncompressed: &[u8]) -> Vec<u8> {
    // Count consecutive identical bytes
    // Encode as (count, value) pairs
    // Use negative count for literal runs
}
```

**Algorithm:**
- Positive count: repeat next byte N times
- Negative count: copy next |N| bytes literally

**Pros:**
- Fast compression/decompression
- Good for simple patterns

**Cons:**
- Poor for complex/noisy images

**Use when:**
- Simple images (solid areas, gradients)
- Speed more important than size

## ZIP / ZIPS

Deflate compression using zlib format.

**ZIP:** 16 scanlines per block
**ZIPS:** 1 scanline per block

```rust
// Compression: miniz_oxide
pub fn compress(data: &[u8]) -> Vec<u8> {
    miniz_oxide::deflate::compress_to_vec_zlib(data, 6)
}

// Decompression: zune-inflate (faster)
pub fn decompress(data: &[u8], size: usize) -> Vec<u8> {
    zune_inflate::DeflateDecoder::new(data)
        .decode_zlib()?
}
```

**Pros:**
- Good compression ratio
- Well-balanced speed
- Universal compatibility

**Cons:**
- Not the smallest files
- Not the fastest

**Use when:**
- General purpose (recommended default)
- Good balance needed

## PIZ

Wavelet-based compression with Huffman coding.

### Pipeline

```
Pixels → Wavelet Transform → Huffman Encode → Output
```

### Wavelet Transform

Haar wavelet on 16-bit values:

```rust
// Horizontal pass
for row in rows {
    for i in (0..width).step_by(2) {
        let a = row[i];
        let b = row[i + 1];
        row[i] = (a + b) / 2;      // Average
        row[i + 1] = a - b;         // Difference
    }
}
// Vertical pass similarly
```

### Huffman Coding

Custom Huffman implementation for 16-bit symbols:

```rust
// Build frequency table
// Generate canonical Huffman codes
// Encode symbols
```

**Pros:**
- Excellent compression for noisy images
- Better than ZIP for film grain, noise

**Cons:**
- Slower than ZIP
- 32-line blocks (more memory)

**Use when:**
- Film grain or noise present
- Compression ratio critical
- CPU time acceptable

## PXR24

Pixar's 24-bit float compression.

### Algorithm

1. Convert f32 to 24-bit (truncate mantissa)
2. ZIP compress the result

```rust
pub fn compress(data: &[u8], channels: &ChannelList) -> Vec<u8> {
    for sample in samples {
        match channel.sample_type {
            F32 => {
                // Truncate to 24 bits (lose 8 bits precision)
                let bits = sample.to_bits();
                let truncated = bits & 0xFFFFFF00;
            }
            F16 | U32 => {
                // Keep full precision
            }
        }
    }
    zip::compress(&truncated)
}
```

**Pros:**
- Good compression for f32 images
- Fast (just ZIP + truncation)
- Lossless for f16 and u32

**Cons:**
- Lossy for f32 (8 bits precision lost)
- Not great for f16 images

**Use when:**
- Large f32 images
- Slight quality loss acceptable
- Good speed needed

## B44 / B44A

Block-based compression for 16-bit floats.

### Algorithm

Divides image into 4x4 blocks, compresses each to fixed size:

```rust
// Each 4x4 block (32 bytes) → 14 bytes
// Uses lookup tables for fast encoding/decoding
```

**B44:** All blocks compressed
**B44A:** Flat (constant) blocks stored as single value

### Lookup Tables

The `table.rs` file contains 10,928 lines of precomputed values for fast encoding.

**Pros:**
- Fixed compression ratio (predictable)
- Fast decompression (good for playback)
- B44A handles flat areas well

**Cons:**
- Lossy compression
- Only works with f16
- Visible artifacts on gradients

**Use when:**
- Real-time playback needed
- Texture compression
- Fixed bitrate required

## DWAA / DWAB (Not Implemented)

DCT-based lossy compression.

**DWAA:** 32 scanlines per block
**DWAB:** 256 scanlines per block

**Status:** Not yet implemented in exrs.

## Deep Data Compression

Deep data uses modified compression:

### Sample Table

Cumulative sample counts per pixel:

```rust
pub fn compress_sample_table(
    compression: Compression,
    counts: &[i32]
) -> Vec<u8> {
    match compression {
        Uncompressed => counts.to_bytes(),
        RLE => rle::compress(&counts.to_bytes()),
        ZIP | ZIPS => zip::compress(&counts.to_bytes()),
        _ => unsupported!()
    }
}
```

### Sample Data

Per-channel data compressed separately:

```rust
pub fn compress_sample_data(
    compression: Compression,
    channels: &[DeepChannelData]
) -> Vec<u8> {
    // Interleave by pixel
    // Apply standard compression
}
```

### Supported for Deep

| Method | Sample Table | Sample Data |
|--------|--------------|-------------|
| Uncompressed | Yes | Yes |
| RLE | Yes | Yes |
| ZIPS | Yes | Yes |
| ZIP | Yes | Yes |
| PIZ | No | No |
| PXR24 | No | No |
| B44/B44A | No | No |

## Choosing Compression

### Decision Tree

```
Is speed critical?
├── Yes → Uncompressed
└── No
    │
    Is it deep data?
    ├── Yes → ZIP (ZIPS for streaming)
    └── No
        │
        Is the image noisy/grainy?
        ├── Yes → PIZ
        └── No
            │
            Is it mostly f32?
            ├── Yes → PXR24 (if loss OK) or ZIP
            └── No
                │
                Need fixed bitrate?
                ├── Yes → B44/B44A
                └── No → ZIP (default)
```

### Quick Reference

| Scenario | Recommendation |
|----------|----------------|
| General purpose | ZIP |
| Maximum speed | Uncompressed |
| Film/VFX (noisy) | PIZ |
| Real-time playback | B44/B44A |
| Deep data | ZIP |
| Maximum compression | PIZ |
| Large f32 images | PXR24 |

## Implementation Details

### Memory Usage

Each compression method has different memory characteristics:

| Method | Temp Memory | Block Size |
|--------|-------------|------------|
| Uncompressed | None | 1 line |
| RLE | 2x block | 1 line |
| ZIP | ~1MB | 16 lines |
| PIZ | 2x block + tables | 32 lines |
| PXR24 | 2x block | 16 lines |
| B44 | Tables (~40KB) | 32 lines |

### Thread Safety

All compression functions are stateless and thread-safe. The library uses rayon for parallel compression/decompression.

## See Also

- [Data Flow](./dataflow.md) - How compression fits in the pipeline
- [Architecture](./architecture.md) - Overall library design
