# EXRS Architecture & Dataflow Documentation

## Module Hierarchy

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
│   └── attribute.rs    # ChannelList, Compression, TileDescription, Text, etc.
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
│   ├── mod.rs          # Compression enum, compress/decompress dispatch
│   ├── rle.rs          # Run-length encoding
│   ├── zip.rs          # ZIP (miniz_oxide + zune-inflate)
│   ├── pxr24.rs        # PXR24 (float24 + ZIP)
│   ├── piz/            # PIZ compression
│   │   ├── mod.rs      # Main compress/decompress
│   │   ├── wavelet.rs  # Haar wavelet transform
│   │   └── huffman.rs  # Huffman coding
│   └── b44/            # B44/B44A compression
│       ├── mod.rs      # Block-based f16 compression
│       └── table.rs    # Lookup tables
│
└── image/              # High-level image API
    ├── mod.rs          # Image, Layer, Levels, Encoding, SpecificChannels, AnyChannels
    ├── read/           # Reading pipeline
    │   ├── mod.rs      # read(), convenience functions
    │   ├── image.rs    # ReadImage, ImageReader
    │   ├── layers.rs   # ReadLayers, LayersReader
    │   ├── specific_channels.rs  # SpecificChannelsReader
    │   ├── any_channels.rs       # AnyChannelsReader
    │   ├── levels.rs   # ReadAllLevels, ReadLargestLevel
    │   ├── samples.rs  # ReadFlatSamples
    │   └── deep.rs     # Deep data reading
    ├── write/          # Writing pipeline
    │   ├── mod.rs      # WritableImage, WriteImageWithOptions
    │   ├── layers.rs   # WritableLayers, LayersWriter
    │   ├── channels.rs # WritableChannels, ChannelsWriter
    │   ├── samples.rs  # WritableSamples, SamplesWriter
    │   └── deep.rs     # Deep data writing
    ├── crop.rs         # CropResult, cropping utilities
    ├── deep.rs         # DeepSamples, DeepChannelData
    ├── pixel_vec.rs    # PixelVec simple storage
    ├── recursive.rs    # Recursive type helpers
    └── channel_groups.rs # Channel grouping utilities
```

---

## Dataflow Diagrams

### 1. Reading Pipeline

```
                                    ┌─────────────────────────────────────────┐
                                    │              USER CODE                   │
                                    │  read().no_deep_data().all_channels()... │
                                    └─────────────────────────────────────────┘
                                                        │
                                                        ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│                             read/mod.rs                                       │
│  ┌─────────────────┐    ┌─────────────────┐    ┌──────────────────────┐      │
│  │ ReadBuilder     │───▶│ ReadFlatSamples │───▶│ ReadSpecificChannels │      │
│  │ (entry point)   │    │ (no deep data)  │    │ or ReadAnyChannels   │      │
│  └─────────────────┘    └─────────────────┘    └──────────────────────┘      │
│                                                           │                   │
│  ┌──────────────────────────────────────────────────────▼─────────────────┐  │
│  │ ReadLargestLevel / ReadAllLevels                                        │  │
│  └─────────────────────────────────────────────────────────────────────────┘  │
│                                                           │                   │
│  ┌──────────────────────────────────────────────────────▼─────────────────┐  │
│  │ ReadFirstValidLayer / ReadAllLayers                                     │  │
│  └─────────────────────────────────────────────────────────────────────────┘  │
│                                                           │                   │
│  ┌──────────────────────────────────────────────────────▼─────────────────┐  │
│  │ ReadImage::from_file() / from_buffered()                                │  │
│  └─────────────────────────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────────────────────────┘
                                                        │
                                                        ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│                            block/reader.rs                                    │
│                                                                               │
│  ┌─────────────────────────────────────────────────────────────────────────┐ │
│  │  Reader::read_from_buffered(read, pedantic)                              │ │
│  │    1. Validate magic number                                              │ │
│  │    2. Read Requirements                                                  │ │
│  │    3. Read Headers (one per layer)                                       │ │
│  │    4. Read/Skip OffsetTables                                             │ │
│  └─────────────────────────────────────────────────────────────────────────┘ │
│                                  │                                            │
│                                  ▼                                            │
│  ┌─────────────────────────────────────────────────────────────────────────┐ │
│  │  reader.all_chunks(pedantic) -> ChunksReader                             │ │
│  │    - Returns iterator over Chunk (compressed blocks)                     │ │
│  └─────────────────────────────────────────────────────────────────────────┘ │
│                                  │                                            │
│                    ┌─────────────┼─────────────┐                              │
│                    ▼             ▼             ▼                              │
│            ┌───────────┐  ┌───────────┐  ┌───────────┐                       │
│            │Sequential │  │ Parallel  │  │ Filtering │                       │
│            │ Reading   │  │ (rayon)   │  │ by offset │                       │
│            └───────────┘  └───────────┘  └───────────┘                       │
└──────────────────────────────────────────────────────────────────────────────┘
                                        │
                                        ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│                          compression/mod.rs                                   │
│                                                                               │
│  ┌─────────────────────────────────────────────────────────────────────────┐ │
│  │  Compression::decompress_image_section_from_le()                         │ │
│  │                                                                          │ │
│  │    match self {                                                          │ │
│  │      Uncompressed => just convert endianness                             │ │
│  │      RLE          => rle::decompress()                                   │ │
│  │      ZIP1/ZIP16   => zip::decompress()                                   │ │
│  │      PIZ          => piz::decompress()                                   │ │
│  │      PXR24        => pxr24::decompress()                                 │ │
│  │      B44/B44A     => b44::decompress()                                   │ │
│  │      DWAA/DWAB    => unsupported error                                   │ │
│  │    }                                                                     │ │
│  └─────────────────────────────────────────────────────────────────────────┘ │
│                                  │                                            │
│                                  ▼                                            │
│  ┌─────────────────────────────────────────────────────────────────────────┐ │
│  │  Post-processing:                                                        │ │
│  │    1. differences_to_samples() - reconstruct from deltas                 │ │
│  │    2. interleave_byte_blocks() - reorder bytes                           │ │
│  │    3. Convert little-endian to native-endian                             │ │
│  └─────────────────────────────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────────────────────────┘
                                        │
                                        ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│                            block/mod.rs                                       │
│                                                                               │
│  ┌─────────────────────────────────────────────────────────────────────────┐ │
│  │  UncompressedBlock::decompress_chunk(chunk, meta_data, pedantic)         │ │
│  │    Returns: UncompressedBlock { index: BlockIndex, data: Vec<u8> }       │ │
│  └─────────────────────────────────────────────────────────────────────────┘ │
│                                  │                                            │
│                                  ▼                                            │
│  ┌─────────────────────────────────────────────────────────────────────────┐ │
│  │  block.lines(channels) -> Iterator<LineRef>                              │ │
│  │    - Yields lines for each channel, each scan line                       │ │
│  └─────────────────────────────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────────────────────────┘
                                        │
                                        ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│                      image/read/specific_channels.rs                          │
│                                                                               │
│  ┌─────────────────────────────────────────────────────────────────────────┐ │
│  │  SpecificChannelsReader::filter_block()                                  │ │
│  │    - Reads line samples into user storage                                │ │
│  │    - Calls user's set_pixel() callback                                   │ │
│  └─────────────────────────────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────────────────────────┘
                                        │
                                        ▼
                              ┌───────────────────┐
                              │  Image<Layers>    │
                              │  (User receives)  │
                              └───────────────────┘
```

### 2. Writing Pipeline

```
┌───────────────────────────────────────────────────────────────────────────────┐
│                              USER CODE                                         │
│  let image = Image::from_channels(...);                                        │
│  image.write().to_file("output.exr")?;                                         │
└───────────────────────────────────────────────────────────────────────────────┘
                                        │
                                        ▼
┌───────────────────────────────────────────────────────────────────────────────┐
│                           image/write/mod.rs                                   │
│                                                                                │
│  ┌──────────────────────────────────────────────────────────────────────────┐ │
│  │  WriteImageWithOptions::to_buffered(write)                                │ │
│  │    1. headers = self.infer_meta_data()                                    │ │
│  │    2. layers = self.image.create_writer(&headers)                         │ │
│  │    3. block::write(write, headers, checks, |meta, chunk_writer| { ... })  │ │
│  └──────────────────────────────────────────────────────────────────────────┘ │
└───────────────────────────────────────────────────────────────────────────────┘
                                        │
                                        ▼
┌───────────────────────────────────────────────────────────────────────────────┐
│                           block/writer.rs                                      │
│                                                                                │
│  ┌──────────────────────────────────────────────────────────────────────────┐ │
│  │  block::write(write, headers, checks, callback)                           │ │
│  │    1. MetaData::write_validating_to_buffered()  - Write magic + headers   │ │
│  │    2. Write placeholder offset table                                      │ │
│  │    3. Create ChunkWriter                                                  │ │
│  │    4. Call user callback to write chunks                                  │ │
│  │    5. Seek back and write final offset table                              │ │
│  └──────────────────────────────────────────────────────────────────────────┘ │
│                                        │                                       │
│  ┌─────────────────────────────────────▼────────────────────────────────────┐ │
│  │  ChunksWriter (wraps ChunkWriter)                                         │ │
│  │    - compress_all_blocks_parallel() - uses rayon thread pool              │ │
│  │    - compress_all_blocks_sequential()                                     │ │
│  └──────────────────────────────────────────────────────────────────────────┘ │
└───────────────────────────────────────────────────────────────────────────────┘
                                        │
                                        ▼
┌───────────────────────────────────────────────────────────────────────────────┐
│                          image/write/layers.rs                                 │
│                                                                                │
│  ┌──────────────────────────────────────────────────────────────────────────┐ │
│  │  LayersWriter::extract_uncompressed_block(headers, block_index)           │ │
│  │    - Dispatches to appropriate layer's channel writer                     │ │
│  └──────────────────────────────────────────────────────────────────────────┘ │
└───────────────────────────────────────────────────────────────────────────────┘
                                        │
                                        ▼
┌───────────────────────────────────────────────────────────────────────────────┐
│                         image/write/channels.rs                                │
│                                                                                │
│  ┌──────────────────────────────────────────────────────────────────────────┐ │
│  │  ChannelsWriter::extract_uncompressed_block()                             │ │
│  │    - Calls UncompressedBlock::from_lines()                                │ │
│  │    - Iterates channels, calls user's get_pixel()                          │ │
│  └──────────────────────────────────────────────────────────────────────────┘ │
└───────────────────────────────────────────────────────────────────────────────┘
                                        │
                                        ▼
┌───────────────────────────────────────────────────────────────────────────────┐
│                            block/mod.rs                                        │
│                                                                                │
│  ┌──────────────────────────────────────────────────────────────────────────┐ │
│  │  UncompressedBlock::compress_to_chunk(headers)                            │ │
│  │    1. Get header for this block's layer                                   │ │
│  │    2. header.compression.compress_image_section_to_le()                   │ │
│  │    3. Return Chunk { layer_index, compressed_block }                      │ │
│  └──────────────────────────────────────────────────────────────────────────┘ │
└───────────────────────────────────────────────────────────────────────────────┘
                                        │
                                        ▼
┌───────────────────────────────────────────────────────────────────────────────┐
│                         compression/mod.rs                                     │
│                                                                                │
│  ┌──────────────────────────────────────────────────────────────────────────┐ │
│  │  Compression::compress_image_section_to_le()                              │ │
│  │                                                                           │ │
│  │    Pre-processing:                                                        │ │
│  │      1. Convert native-endian to little-endian                            │ │
│  │      2. separate_bytes_fragments() - reorder bytes                        │ │
│  │      3. samples_to_differences() - compute deltas                         │ │
│  │                                                                           │ │
│  │    match self {                                                           │ │
│  │      Uncompressed => just return LE bytes                                 │ │
│  │      RLE          => rle::compress()                                      │ │
│  │      ZIP1/ZIP16   => zip::compress()                                      │ │
│  │      PIZ          => piz::compress()                                      │ │
│  │      PXR24        => pxr24::compress()                                    │ │
│  │      B44/B44A     => b44::compress()                                      │ │
│  │    }                                                                      │ │
│  └──────────────────────────────────────────────────────────────────────────┘ │
└───────────────────────────────────────────────────────────────────────────────┘
                                        │
                                        ▼
                              ┌───────────────────┐
                              │  .exr File        │
                              │  (Written)        │
                              └───────────────────┘
```

### 3. Deep Data Pipeline

```
┌───────────────────────────────────────────────────────────────────────────────┐
│                        Deep Data Reading                                       │
│                                                                                │
│  read().deep_data().all_channels().first_valid_layer().from_file()             │
│                                        │                                       │
│                                        ▼                                       │
│  ┌──────────────────────────────────────────────────────────────────────────┐ │
│  │  image/read/deep.rs                                                       │ │
│  │                                                                           │ │
│  │  ReadDeepImage::from_buffered()                                           │ │
│  │    1. Reader::read_from_buffered()                                        │ │
│  │    2. Find first deep layer (header.deep == true)                         │ │
│  │    3. read_deep_layer_internal()                                          │ │
│  │       a. reader.all_chunks()                                              │ │
│  │       b. decompress_blocks_parallel/sequential()                          │ │
│  │       c. Sort blocks by y-coordinate                                      │ │
│  │       d. merge_deep_blocks() - combine into single DeepSamples            │ │
│  │    4. build_deep_layer()                                                  │ │
│  └──────────────────────────────────────────────────────────────────────────┘ │
│                                        │                                       │
│                                        ▼                                       │
│  ┌──────────────────────────────────────────────────────────────────────────┐ │
│  │  block/deep.rs                                                            │ │
│  │                                                                           │ │
│  │  decompress_deep_scanline_block() / decompress_deep_tile_block()          │ │
│  │    1. Decompress sample count table (ZIP)                                 │ │
│  │    2. Validate cumulative counts                                          │ │
│  │    3. Decompress sample data per compression type                         │ │
│  │    4. Unpack channel data into DeepSamples                                │ │
│  └──────────────────────────────────────────────────────────────────────────┘ │
│                                        │                                       │
│                                        ▼                                       │
│  ┌──────────────────────────────────────────────────────────────────────────┐ │
│  │  image/deep.rs                                                            │ │
│  │                                                                           │ │
│  │  DeepSamples {                                                            │ │
│  │    sample_offsets: Vec<u32>,    // Cumulative counts per pixel            │ │
│  │    channels: Vec<DeepChannelData>, // SoA layout                          │ │
│  │    width, height                                                          │ │
│  │  }                                                                        │ │
│  │                                                                           │ │
│  │  Methods: sample_count(), sample_range(), pixels(), pixel()               │ │
│  └──────────────────────────────────────────────────────────────────────────┘ │
└───────────────────────────────────────────────────────────────────────────────┘
```

---

## Key Type Relationships

```
Image<Layers>
  ├── attributes: ImageAttributes
  │     ├── display_window: IntegerBounds
  │     ├── pixel_aspect: f32
  │     ├── chromaticities: Option<Chromaticities>
  │     └── time_code: Option<TimeCode>
  │
  └── layer_data: Layers
        │
        ├── [Single Layer] Layer<Channels>
        │     ├── channel_data: Channels
        │     ├── attributes: LayerAttributes
        │     ├── size: Vec2<usize>
        │     └── encoding: Encoding
        │           ├── compression: Compression
        │           ├── line_order: LineOrder
        │           └── blocks: Blocks (ScanLines | Tiles)
        │
        └── [Multiple Layers] SmallVec<[Layer<Channels>; N]>

Channels variants:
  ├── AnyChannels<Samples>
  │     └── list: SmallVec<[AnyChannel<Samples>; 4]>
  │           ├── name: Text
  │           ├── sample_data: Samples
  │           ├── quantize_linearly: bool
  │           └── sampling: Vec2<usize>
  │
  └── SpecificChannels<Pixels, Description>
        ├── channels: Description (e.g., RgbaChannels)
        └── pixels: Pixels (user storage type)

Samples variants:
  ├── FlatSamples (single resolution)
  │     ├── F16(Vec<f16>)
  │     ├── F32(Vec<f32>)
  │     └── U32(Vec<u32>)
  │
  ├── Levels<S> (mip/rip maps)
  │     ├── Singular(S)
  │     ├── Mip { rounding_mode, level_data: LevelMaps<S> }
  │     └── Rip { rounding_mode, level_data: RipMaps<S> }
  │
  └── DeepSamples (variable samples per pixel)
        ├── sample_offsets: Vec<u32>
        ├── channels: Vec<DeepChannelData>
        └── width, height
```

---

## Compression Flow

```
Native Endian Bytes (from user)
         │
         ▼
┌─────────────────────────────────┐
│  Convert to Little Endian       │
│  (sample_to_le for each type)   │
└─────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────┐
│  separate_bytes_fragments()     │
│  Reorder: [AABBCC] → [ABCABC]   │
│  (improves compression ratio)   │
└─────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────┐
│  samples_to_differences()       │
│  Delta encoding: [1,3,6] →      │
│                  [1,2,3]        │
└─────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────┐
│  Compression Algorithm          │
│  (RLE/ZIP/PIZ/PXR24/B44)        │
└─────────────────────────────────┘
         │
         ▼
Compressed Bytes (to file)


Compressed Bytes (from file)
         │
         ▼
┌─────────────────────────────────┐
│  Decompression Algorithm        │
│  (reverse of above)             │
└─────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────┐
│  differences_to_samples()       │
│  Undo delta: [1,2,3] →          │
│              [1,3,6]            │
└─────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────┐
│  interleave_byte_blocks()       │
│  Reorder: [ABCABC] → [AABBCC]   │
└─────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────┐
│  Convert to Native Endian       │
│  (le_to_sample for each type)   │
└─────────────────────────────────┘
         │
         ▼
Native Endian Bytes (to user)
```

---

## Thread Safety & Parallelism

```
Sequential Reading:
  File → Reader → Chunks (one at a time) → Decompress → Assemble

Parallel Reading (with rayon):
  File → Reader → Chunks
                    │
         ┌──────────┴──────────┐
         ▼                     ▼
    Decompress              Decompress
    (thread 1)              (thread 2)
         │                     │
         └──────────┬──────────┘
                    ▼
              mpsc::channel
                    │
                    ▼
              Assemble Image
              (main thread)

Parallel Writing (with rayon):
  Image Data
       │
       ▼
  Generate Blocks
       │
  ┌────┴────┐
  ▼         ▼
Compress  Compress   (rayon parallel iterator)
  │         │
  └────┬────┘
       ▼
  mpsc::channel
       │
       ▼
  Write to File
  (main thread)
```

---

## File Format Structure

```
OpenEXR File Layout:
┌─────────────────────────────────────┐
│ Magic Number (4 bytes)              │  0x762f3101
├─────────────────────────────────────┤
│ Version + Flags (4 bytes)           │  Version 2 + feature bits
├─────────────────────────────────────┤
│ Header 1                            │
│   - Attributes (name=value pairs)   │
│   - 0x00 terminator                 │
├─────────────────────────────────────┤
│ [Header 2...N if multipart]         │
│   - 0x00 terminator after last      │
├─────────────────────────────────────┤
│ Offset Table 1                      │  Array of u64 chunk offsets
├─────────────────────────────────────┤
│ [Offset Table 2...N if multipart]   │
├─────────────────────────────────────┤
│ Chunk 1                             │
│   - Part number (if multipart)      │
│   - Coordinates (tile or scanline)  │
│   - Compressed pixel data           │
├─────────────────────────────────────┤
│ Chunk 2...M                         │
└─────────────────────────────────────┘
```

---

## Error Handling Strategy

```rust
// Error enum (src/error.rs)
pub enum Error {
    NotSupported(Cow<'static, str>),  // Feature not implemented
    Invalid(Cow<'static, str>),       // Malformed data
    Io(IoError),                      // I/O errors
    Aborted,                          // Operation cancelled (unused?)
}

// Result types
pub type Result<T> = std::result::Result<T, Error>;
pub type UnitResult = Result<()>;

// Helper functions
pub fn usize_to_i32(value: usize, name: &str) -> Result<i32>
pub fn i32_to_usize(value: i32, name: &str) -> Result<usize>
```

---

*Documentation generated: 2026-01-05*
