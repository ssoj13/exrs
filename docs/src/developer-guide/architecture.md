# Architecture Overview

This document describes the high-level architecture of the exrs library.

## Design Philosophy

### Safety First

The library uses no unsafe code:

```rust
#![forbid(unsafe_code)]
```

All operations are bounds-checked, and allocations have safe maximum sizes to prevent memory exhaustion attacks.

### Layered Abstraction

```
┌─────────────────────────────────────────┐
│           User Code                      │
├─────────────────────────────────────────┤
│    High-Level Image API (image/)         │  ← Most users stop here
├─────────────────────────────────────────┤
│    Block API (block/)                    │  ← For custom processing
├─────────────────────────────────────────┤
│    Metadata (meta/)                      │  ← Headers, attributes
├─────────────────────────────────────────┤
│    Compression (compression/)            │  ← Algorithm implementations
├─────────────────────────────────────────┤
│    I/O Utilities (io.rs)                 │  ← Reading/writing primitives
└─────────────────────────────────────────┘
```

### Type-Driven API

The image type reflects your choices at compile time:

```rust
// Your choices...
read()
    .no_deep_data()           // FlatSamples vs DeepSamples
    .largest_resolution_level() // vs Levels<>
    .all_channels()           // AnyChannels vs SpecificChannels
    .first_valid_layer()      // Layer vs Layers
    .all_attributes()

// ...determine the result type
Image<Layer<AnyChannels<FlatSamples>>>
```

## Module Structure

```
exrs/
├── lib.rs              # Entry point, prelude exports
├── error.rs            # Error types (Error, Result, UnitResult)
├── math.rs             # Vec2<T>, RoundingMode
├── io.rs               # PeekRead, Tracking, Data trait
│
├── meta/               # File metadata
│   ├── mod.rs          # MetaData, Requirements, BlockDescription
│   ├── header.rs       # Header, ImageAttributes, LayerAttributes
│   └── attribute.rs    # ChannelList, Compression, Text, etc.
│
├── block/              # Low-level block I/O
│   ├── mod.rs          # BlockIndex, UncompressedBlock
│   ├── reader.rs       # Reader<R>, ChunksReader
│   ├── writer.rs       # ChunkWriter<W>, ChunksWriter
│   ├── chunk.rs        # Chunk, CompressedBlock variants
│   ├── lines.rs        # LineIndex, LineRef, LineRefMut
│   ├── samples.rs      # Sample enum (F16/F32/U32)
│   └── deep.rs         # DeepUncompressedBlock, decompressors
│
├── compression/        # Compression algorithms
│   ├── mod.rs          # Compression enum, dispatch
│   ├── rle.rs          # Run-length encoding
│   ├── zip.rs          # ZIP (miniz_oxide + zune-inflate)
│   ├── pxr24.rs        # PXR24 (float24 + ZIP)
│   ├── piz/            # PIZ compression
│   │   ├── mod.rs
│   │   ├── wavelet.rs
│   │   └── huffman.rs
│   └── b44/            # B44/B44A compression
│       ├── mod.rs
│       └── table.rs
│
└── image/              # High-level image API
    ├── mod.rs          # Image, Layer, Encoding, Channels
    ├── deep.rs         # DeepSamples, DeepChannelData
    ├── crop.rs         # Cropping utilities
    ├── pixel_vec.rs    # Simple pixel storage
    ├── recursive.rs    # Recursive type helpers
    ├── channel_groups.rs
    │
    ├── read/           # Reading pipeline
    │   ├── mod.rs      # read(), convenience functions
    │   ├── image.rs    # ReadImage trait
    │   ├── layers.rs   # Layer reading
    │   ├── levels.rs   # Resolution level reading
    │   ├── samples.rs  # Sample reading
    │   ├── any_channels.rs
    │   ├── specific_channels.rs
    │   ├── any_samples.rs  # Unified deep/flat
    │   └── deep.rs     # Deep-specific reading
    │
    └── write/          # Writing pipeline
        ├── mod.rs      # WritableImage trait
        ├── layers.rs
        ├── channels.rs
        ├── samples.rs
        └── deep.rs     # Deep-specific writing
```

## Key Abstractions

### Image\<Layers\>

The top-level container:

```rust
pub struct Image<Layers> {
    pub attributes: ImageAttributes,
    pub layer_data: Layers,
}
```

`Layers` can be:
- `Layer<Channels>` - Single layer
- `Layers<Channels>` (SmallVec) - Multiple layers

### Layer\<Channels\>

A single image layer:

```rust
pub struct Layer<Channels> {
    pub channel_data: Channels,
    pub attributes: LayerAttributes,
    pub size: Vec2<usize>,
    pub encoding: Encoding,
}
```

### Channels

Two variants:

**SpecificChannels** - Known channels at compile time:
```rust
pub struct SpecificChannels<Pixels, ChannelDescriptions> {
    pub channels: ChannelDescriptions,  // (R, G, B, A) descriptions
    pub pixels: Pixels,                  // Your storage
}
```

**AnyChannels** - Dynamic channel list:
```rust
pub struct AnyChannels<Samples> {
    pub list: SmallVec<[AnyChannel<Samples>; 4]>,
}
```

### Samples

Flat samples:
```rust
pub enum FlatSamples {
    F16(Vec<f16>),
    F32(Vec<f32>),
    U32(Vec<u32>),
}
```

Deep samples:
```rust
pub struct DeepSamples {
    pub sample_offsets: Vec<u32>,
    pub channels: Vec<DeepChannelData>,
    pub width: usize,
    pub height: usize,
}
```

### Encoding

How pixels are stored:
```rust
pub struct Encoding {
    pub compression: Compression,
    pub blocks: Blocks,
    pub line_order: LineOrder,
}
```

## Error Handling

```rust
pub enum Error {
    NotSupported(Cow<'static, str>),  // Feature not implemented
    Invalid(Cow<'static, str>),       // Malformed data
    Io(IoError),                      // I/O errors
    Aborted,                          // Operation cancelled
}

pub type Result<T> = std::result::Result<T, Error>;
pub type UnitResult = Result<()>;
```

## Thread Safety

The library is designed for parallel processing:

```rust
// Reading: parallel decompression
read()
    .no_deep_data()
    // ... options ...
    .from_file("image.exr")  // Uses rayon internally

// Writing: parallel compression
image.write()
    .to_file("output.exr")   // Uses rayon internally

// Disable if needed
.non_parallel()
```

Parallelism is opt-out via the `rayon` feature flag.

## Memory Strategy

### Allocation Limits

To prevent memory exhaustion:
```rust
const MAX_ALLOCATION_SIZE: usize = 1024 * 1024 * 1024;  // 1 GB
```

### SmallVec Usage

For common small collections:
```rust
pub type Layers<C> = SmallVec<[Layer<C>; 2]>;
pub type ChannelList = SmallVec<[AnyChannel; 4]>;
```

### Zero-Copy Where Possible

The reader uses `std::io::Read` trait, allowing:
- Memory-mapped files
- Network streams
- In-memory buffers

## Extension Points

### Custom Pixel Storage

Implement `GetPixel` for writing:
```rust
trait GetPixel {
    type Pixel;
    fn get_pixel(&self, position: Vec2<usize>) -> Self::Pixel;
}
```

### Custom Attribute Types

Use `AttributeValue::Custom` for unknown types:
```rust
AttributeValue::Custom {
    type_name: Text,
    bytes: Vec<u8>,
}
```

### Low-Level Block Access

For custom processing:
```rust
use exr::block;

// Read blocks directly
let reader = block::read(...)?;
for chunk in reader.all_chunks(pedantic) {
    let block = UncompressedBlock::decompress_chunk(chunk?, &meta, pedantic)?;
    // Custom processing
}
```

## See Also

- [Module Structure](./modules.md) - Detailed module documentation
- [Data Flow](./dataflow.md) - How data moves through the library
- [Compression](./compression.md) - Compression algorithm details
