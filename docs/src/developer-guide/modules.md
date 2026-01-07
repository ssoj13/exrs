# Module Structure

Detailed documentation of each module in exrs.

## `lib.rs` - Entry Point

The crate root exports:

```rust
pub mod io;           // I/O primitives
pub mod compression;  // Compression algorithms
pub mod image;        // High-level API
pub mod math;         // Vec2, etc.
pub mod meta;         // Metadata types
pub mod block;        // Low-level blocks
pub mod error;        // Error types

pub mod prelude;      // Convenient re-exports
```

## `error.rs` - Error Types

### Error Enum

```rust
pub enum Error {
    /// Feature not yet implemented
    NotSupported(Cow<'static, str>),
    
    /// Invalid/malformed data
    Invalid(Cow<'static, str>),
    
    /// I/O error (wraps std::io::Error)
    Io(IoError),
    
    /// Operation aborted
    Aborted,
}
```

### Helper Functions

```rust
pub fn usize_to_i32(value: usize, name: &str) -> Result<i32>
pub fn i32_to_usize(value: i32, name: &str) -> Result<usize>
```

## `math.rs` - Math Utilities

### Vec2

Generic 2D vector:

```rust
pub struct Vec2<T>(pub T, pub T);

impl<T> Vec2<T> {
    pub fn x(&self) -> T;
    pub fn y(&self) -> T;
    pub fn width(&self) -> T;   // Alias for x
    pub fn height(&self) -> T;  // Alias for y
    pub fn area(&self) -> T;    // x * y
}
```

### RoundingMode

For mipmap calculations:

```rust
pub enum RoundingMode {
    Up,
    Down,
}
```

## `io.rs` - I/O Utilities

### PeekRead

Buffered reading with peek:

```rust
pub trait PeekRead: Read {
    fn peek(&mut self, count: usize) -> &[u8];
}
```

### Tracking

Tracks bytes read/written:

```rust
pub struct Tracking<T> {
    inner: T,
    position: u64,
}
```

### Data Trait

Binary serialization:

```rust
pub trait Data: Sized {
    fn read(read: &mut impl Read) -> Result<Self>;
    fn write(&self, write: &mut impl Write) -> UnitResult;
}
```

## `meta/` - Metadata

### `meta/mod.rs`

Core metadata types:

```rust
pub struct MetaData {
    pub requirements: Requirements,
    pub headers: Headers,
}

pub struct Requirements {
    pub file_format_version: u8,
    pub is_single_part: bool,
    pub has_deep_data: bool,
    // ...
}
```

### `meta/header.rs`

Header structures:

```rust
pub struct Header {
    pub channels: ChannelList,
    pub compression: Compression,
    pub data_size: Vec2<usize>,
    pub blocks: BlockDescription,
    pub layer_attributes: LayerAttributes,
    pub shared_attributes: Option<Arc<ImageAttributes>>,
    // ...
}

pub struct ImageAttributes {
    pub display_window: IntegerBounds,
    pub pixel_aspect: f32,
    pub chromaticities: Option<Chromaticities>,
    pub time_code: Option<TimeCode>,
    pub other: HashMap<Text, AttributeValue>,
}

pub struct LayerAttributes {
    pub layer_name: Option<Text>,
    pub owner: Option<Text>,
    pub comments: Option<Text>,
    pub software_name: Option<Text>,
    // ...many more
}
```

### `meta/attribute.rs`

Attribute types:

```rust
pub struct Text(SmallVec<[u8; 24]>);  // UTF-8 string

pub enum Compression {
    Uncompressed,
    RLE,
    ZIPS,
    ZIP,
    PIZ,
    PXR24,
    B44,
    B44A,
    DWAA,
    DWAB,
}

pub enum SampleType {
    U32,
    F16,
    F32,
}

pub struct ChannelDescription {
    pub name: Text,
    pub sample_type: SampleType,
    pub quantize_linearly: bool,
    pub sampling: Vec2<usize>,
}

pub enum AttributeValue {
    I32(i32),
    F32(f32),
    F64(f64),
    Text(Text),
    // ... many more
    Custom { type_name: Text, bytes: Vec<u8> },
}
```

## `block/` - Block I/O

### `block/mod.rs`

Core block types:

```rust
pub struct BlockIndex {
    pub layer: usize,
    pub level: Vec2<usize>,
    pub pixel_position: Vec2<usize>,
    pub pixel_size: Vec2<usize>,
}

pub struct UncompressedBlock {
    pub index: BlockIndex,
    pub data: Vec<u8>,
}
```

### `block/chunk.rs`

Compressed chunks:

```rust
pub struct Chunk {
    pub layer_index: usize,
    pub compressed_block: CompressedBlock,
}

pub enum CompressedBlock {
    ScanLine { y: i32, data: Vec<u8> },
    Tile { position: Vec2<usize>, level: Vec2<usize>, data: Vec<u8> },
    DeepScanLine { y: i32, sample_table: Vec<u8>, sample_data: Vec<u8> },
    DeepTile { ... },
}
```

### `block/reader.rs`

Reading:

```rust
pub struct Reader<R> {
    meta_data: MetaData,
    read: R,
    // ...
}

impl<R: Read + Seek> Reader<R> {
    pub fn read_from_file(path: impl AsRef<Path>) -> Result<Self>;
    pub fn read_from_buffered(read: R, pedantic: bool) -> Result<Self>;
    pub fn all_chunks(self, pedantic: bool) -> ChunksReader<R>;
}
```

### `block/writer.rs`

Writing:

```rust
pub fn write<W: Write + Seek>(
    write: W,
    headers: Headers,
    write_chunks: impl FnOnce(MetaData, &mut ChunksWriter<W>) -> UnitResult,
) -> UnitResult;
```

### `block/lines.rs`

Line access:

```rust
pub struct LineIndex {
    pub channel: usize,
    pub position: Vec2<usize>,
}

pub struct LineRef<'a> {
    pub index: LineIndex,
    pub data: &'a [u8],
}
```

### `block/samples.rs`

Sample types:

```rust
pub enum Sample {
    F16(f16),
    F32(f32),
    U32(u32),
}
```

### `block/deep.rs`

Deep block processing:

```rust
pub struct DeepUncompressedBlock {
    pub index: BlockIndex,
    pub samples: DeepSamples,
}

pub fn decompress_deep_scanline_block(...) -> Result<DeepUncompressedBlock>;
pub fn decompress_deep_tile_block(...) -> Result<DeepUncompressedBlock>;
```

## `compression/` - Algorithms

### `compression/mod.rs`

Dispatch:

```rust
impl Compression {
    pub fn compress_image_section_to_le(&self, ...) -> Result<Vec<u8>>;
    pub fn decompress_image_section_from_le(&self, ...) -> Result<Vec<u8>>;
}

// Pre/post processing
fn separate_bytes_fragments(bytes: &mut [u8]);
fn interleave_byte_blocks(bytes: &mut [u8]);
fn samples_to_differences(bytes: &mut [u8]);
fn differences_to_samples(bytes: &mut [u8]);
```

### `compression/rle.rs`

Run-length encoding:

```rust
pub fn compress(uncompressed: &[u8]) -> Result<Vec<u8>>;
pub fn decompress(compressed: &[u8], expected_size: usize) -> Result<Vec<u8>>;
```

### `compression/zip.rs`

ZIP compression (using miniz_oxide + zune-inflate):

```rust
pub fn compress(uncompressed: &[u8]) -> Result<Vec<u8>>;
pub fn decompress(compressed: &[u8], expected_size: usize) -> Result<Vec<u8>>;
```

### `compression/pxr24.rs`

PXR24 (24-bit float):

```rust
pub fn compress(channels: &ChannelList, uncompressed: &[u8]) -> Result<Vec<u8>>;
pub fn decompress(channels: &ChannelList, compressed: &[u8]) -> Result<Vec<u8>>;
```

### `compression/piz/`

PIZ compression (Huffman + wavelet):

- `mod.rs` - Main compress/decompress
- `huffman.rs` - Huffman coding
- `wavelet.rs` - Haar wavelet transform

### `compression/b44/`

B44 block compression:

- `mod.rs` - Block processing
- `table.rs` - Lookup tables (10,928 lines)

## `image/` - High-Level API

### `image/mod.rs`

Core types:

```rust
pub struct Image<Layers> {
    pub attributes: ImageAttributes,
    pub layer_data: Layers,
}

pub struct Layer<Channels> {
    pub channel_data: Channels,
    pub attributes: LayerAttributes,
    pub size: Vec2<usize>,
    pub encoding: Encoding,
}

pub struct Encoding {
    pub compression: Compression,
    pub blocks: Blocks,
    pub line_order: LineOrder,
}
```

### `image/deep.rs`

Deep data types:

```rust
pub struct DeepSamples {
    pub sample_offsets: Vec<u32>,
    pub channels: Vec<DeepChannelData>,
    pub width: usize,
    pub height: usize,
}

impl DeepSamples {
    pub fn total_samples(&self) -> usize;
    pub fn sample_count(&self, x: usize, y: usize) -> usize;
    pub fn sample_range(&self, pixel_idx: usize) -> (usize, usize);
    pub fn max_samples_per_pixel(&self) -> usize;
}
```

### `image/read/`

Reading pipeline:

- `mod.rs` - `read()` entry point, convenience functions
- `image.rs` - `ReadImage` trait
- `layers.rs` - Layer reading
- `levels.rs` - Resolution levels, `LevelInfo`
- `samples.rs` - Sample reading
- `any_channels.rs` - Dynamic channels
- `specific_channels.rs` - Static channels
- `any_samples.rs` - Unified deep/flat
- `deep.rs` - Deep-specific reading

### `image/write/`

Writing pipeline:

- `mod.rs` - `WritableImage` trait
- `layers.rs` - Layer writing
- `channels.rs` - Channel writing
- `samples.rs` - Sample writing
- `deep.rs` - Deep-specific writing

### `image/crop.rs`

Cropping utilities:

```rust
pub trait Crop<Sample> {
    type Cropped;
    fn crop(self, crop_where: impl FnMut(&Sample) -> bool) -> CropResult<Self::Cropped>;
}

pub enum CropResult<T> {
    Cropped { result: T, data_window_offset: Vec2<i32> },
    Empty,
}
```

### `image/pixel_vec.rs`

Simple pixel storage:

```rust
pub struct PixelVec<Pixel> {
    pub resolution: Vec2<usize>,
    pub pixels: Vec<Pixel>,
}
```

## See Also

- [Architecture Overview](./architecture.md) - High-level design
- [Data Flow](./dataflow.md) - How data moves through modules
