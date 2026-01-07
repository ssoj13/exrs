# Image Module

The `exr::image` module provides high-level APIs for reading and writing EXR images.

## Core Types

### Image

Top-level container for EXR data:

```rust
pub struct Image<Layers> {
    /// Image-level attributes (display window, etc.)
    pub attributes: ImageAttributes,
    
    /// Layer data (single layer or multiple)
    pub layer_data: Layers,
}

impl<L> Image<L> {
    /// Create from layer data
    pub fn new(attributes: ImageAttributes, layer_data: L) -> Self;
    
    /// Create with default attributes
    pub fn from_layer(layer: L) -> Self;
    
    /// Create with channels directly
    pub fn from_channels(size: impl Into<Vec2<usize>>, channels: C) -> Self;
    
    /// Create empty, add layers with .with_layer()
    pub fn empty(attributes: ImageAttributes) -> Self;
    
    /// Add a layer (returns new Image with different type)
    pub fn with_layer<N>(self, layer: Layer<N>) -> Image<(L, Layer<N>)>;
}
```

### Layer

A single image layer:

```rust
pub struct Layer<Channels> {
    /// Pixel data organized by channels
    pub channel_data: Channels,
    
    /// Layer-specific metadata
    pub attributes: LayerAttributes,
    
    /// Resolution (width, height)
    pub size: Vec2<usize>,
    
    /// How pixels are stored
    pub encoding: Encoding,
}

impl<C> Layer<C> {
    pub fn new(
        size: impl Into<Vec2<usize>>,
        attributes: LayerAttributes,
        encoding: Encoding,
        channels: C,
    ) -> Self;
}
```

### Encoding

Storage format for pixels:

```rust
pub struct Encoding {
    pub compression: Compression,
    pub blocks: Blocks,
    pub line_order: LineOrder,
}

impl Encoding {
    /// Uncompressed
    pub const UNCOMPRESSED: Encoding;
    
    /// ZIP compression (fast + good compression)
    pub const FAST_LOSSLESS: Encoding;
    
    /// PIZ compression (best lossless)
    pub const SMALL_LOSSLESS: Encoding;
    
    /// B44 compression (fast lossy)
    pub const SMALL_LOSSY: Encoding;
}
```

## Channel Types

### AnyChannels

Dynamic list of channels:

```rust
pub struct AnyChannels<Samples> {
    pub list: SmallVec<[AnyChannel<Samples>; 4]>,
}

impl<S> AnyChannels<S> {
    /// Create and sort by name
    pub fn sort(channels: SmallVec<[AnyChannel<S>; 4]>) -> Self;
}
```

### AnyChannel

Single channel with any sample type:

```rust
pub struct AnyChannel<Samples> {
    pub name: Text,
    pub sample_data: Samples,
    pub quantize_linearly: bool,
    pub sampling: Vec2<usize>,
}

impl<S> AnyChannel<S> {
    pub fn new(name: impl Into<Text>, samples: S) -> Self;
}
```

### SpecificChannels

Compile-time known channels:

```rust
pub struct SpecificChannels<Pixels, ChannelDescriptions> {
    pub channels: ChannelDescriptions,
    pub pixels: Pixels,
}

impl SpecificChannels<(), ()> {
    /// Start building specific channels
    pub fn build() -> SpecificChannelsBuilder<(), ()>;
    
    /// RGBA channels
    pub fn rgba<Px>(
        get_pixel: impl Fn(Vec2<usize>) -> Px
    ) -> SpecificChannels<impl GetPixel<Pixel=Px>, RgbaChannels>;
    
    /// RGB channels
    pub fn rgb<Px>(
        get_pixel: impl Fn(Vec2<usize>) -> Px
    ) -> SpecificChannels<impl GetPixel<Pixel=Px>, RgbChannels>;
}
```

### SpecificChannelsBuilder

```rust
impl SpecificChannelsBuilder<...> {
    /// Add a channel
    pub fn with_channel(self, name: impl Into<Text>) -> Self;
    
    /// Finalize with pixel function
    pub fn with_pixel_fn<Px>(
        self,
        get_pixel: impl Fn(Vec2<usize>) -> Px
    ) -> SpecificChannels<...>;
}
```

## Sample Types

### FlatSamples

One sample per pixel:

```rust
pub enum FlatSamples {
    F16(Vec<f16>),
    F32(Vec<f32>),
    U32(Vec<u32>),
}

impl FlatSamples {
    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;
    pub fn sample_type(&self) -> SampleType;
}
```

### Levels

Multi-resolution samples:

```rust
pub enum Levels<S> {
    Singular(S),
    Mip {
        rounding_mode: RoundingMode,
        level_data: LevelMaps<S>,
    },
    Rip {
        rounding_mode: RoundingMode,
        level_data: RipMaps<S>,
    },
}
```

### Sample

Dynamic sample value:

```rust
pub enum Sample {
    F16(f16),
    F32(f32),
    U32(u32),
}

impl Sample {
    pub fn to_f16(self) -> f16;
    pub fn to_f32(self) -> f32;
    pub fn to_u32(self) -> u32;
}

impl From<f16> for Sample { ... }
impl From<f32> for Sample { ... }
impl From<u32> for Sample { ... }
```

## Reading

### read() Entry Point

```rust
pub fn read() -> ReadBuilder;
```

### ReadBuilder

```rust
impl ReadBuilder {
    /// Skip deep data
    pub fn no_deep_data(self) -> ReadFlatSamples;
    
    /// Read deep data
    pub fn any_deep_data(self) -> ReadDeepSamples;
    
    /// Read either deep or flat
    pub fn flat_and_deep_data(self) -> ReadAnySamples;
}
```

### ReadFlatSamples

```rust
impl ReadFlatSamples {
    /// Only largest resolution
    pub fn largest_resolution_level(self) -> ReadLargestLevel;
    
    /// All mip/rip levels
    pub fn all_resolution_levels(self) -> ReadAllLevels;
    
    /// Specific level by selector
    pub fn specific_resolution_level<F>(
        self, 
        selector: F
    ) -> ReadSpecificLevel
    where F: FnOnce(Vec<LevelInfo>) -> Vec2<usize>;
}
```

### ReadChannels

```rust
impl ReadChannels {
    /// All channels dynamically
    pub fn all_channels(self) -> ReadAnyChannels;
    
    /// RGBA with custom storage
    pub fn rgba_channels<C, S>(
        self,
        constructor: C,
        setter: S,
    ) -> ReadRgbaChannels;
    
    /// RGB with custom storage
    pub fn rgb_channels<C, S>(...) -> ReadRgbChannels;
    
    /// Custom channel set
    pub fn specific_channels(self) -> ReadSpecificChannels;
}
```

### ReadLayers

```rust
impl ReadLayers {
    /// First layer matching requirements
    pub fn first_valid_layer(self) -> ReadFirstLayer;
    
    /// All layers
    pub fn all_layers(self) -> ReadAllLayers;
    
    /// All layers matching requirements
    pub fn all_valid_layers(self) -> ReadAllValidLayers;
}
```

### Final Read

```rust
impl ReadImage {
    /// Load all attributes
    pub fn all_attributes(self) -> ReadWithAttributes;
}

impl ReadWithAttributes {
    /// Progress callback
    pub fn on_progress(self, callback: impl FnMut(f64)) -> Self;
    
    /// Disable parallel decompression
    pub fn non_parallel(self) -> Self;
    
    /// Strict validation
    pub fn pedantic(self) -> Self;
    
    /// Read from file
    pub fn from_file(self, path: impl AsRef<Path>) -> Result<Image<...>>;
    
    /// Read from buffered reader
    pub fn from_buffered(self, reader: impl Read) -> Result<Image<...>>;
    
    /// Read from unbuffered reader
    pub fn from_unbuffered(self, reader: impl Read) -> Result<Image<...>>;
}
```

## Writing

### Image::write()

```rust
impl<L: WritableImage> Image<L> {
    pub fn write(self) -> WriteImageWithOptions<L>;
}

impl<L> WriteImageWithOptions<L> {
    /// Progress callback
    pub fn on_progress(self, callback: impl FnMut(f64)) -> Self;
    
    /// Disable parallel compression
    pub fn non_parallel(self) -> Self;
    
    /// Write to file
    pub fn to_file(self, path: impl AsRef<Path>) -> UnitResult;
    
    /// Write to buffered writer
    pub fn to_buffered(self, writer: impl Write + Seek) -> UnitResult;
    
    /// Write to unbuffered writer
    pub fn to_unbuffered(self, writer: impl Write + Seek) -> UnitResult;
}
```

## Cropping

```rust
pub trait Crop<Sample> {
    type Cropped;
    
    fn crop(
        self, 
        should_crop: impl FnMut(&Sample) -> bool
    ) -> CropResult<Self::Cropped>;
}

pub enum CropResult<T> {
    Cropped {
        result: T,
        data_window_offset: Vec2<i32>,
    },
    Empty,
}
```

## Pixel Storage

### PixelVec

Simple pixel storage:

```rust
pub struct PixelVec<Pixel> {
    pub resolution: Vec2<usize>,
    pub pixels: Vec<Pixel>,
}

impl<P: Default + Clone> PixelVec<P> {
    pub fn constructor(resolution: Vec2<usize>, _: &C) -> Self;
    pub fn set_pixel(&mut self, position: Vec2<usize>, pixel: P);
}
```

### GetPixel Trait

For writing:

```rust
pub trait GetPixel {
    type Pixel;
    fn get_pixel(&self, position: Vec2<usize>) -> Self::Pixel;
}
```

## LevelInfo

For resolution level selection:

```rust
pub struct LevelInfo {
    /// Level index (x, y)
    pub index: Vec2<usize>,
    
    /// Resolution at this level
    pub resolution: Vec2<usize>,
}
```

## See Also

- [Prelude](./prelude.md) - Common re-exports
- [Meta Module](./meta.md) - Metadata types
- [Deep Data API](./deep.md) - Deep data specifics
