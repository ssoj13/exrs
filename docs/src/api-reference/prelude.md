# Prelude

The `exr::prelude` module re-exports the most commonly used types and functions.

## Usage

```rust
use exr::prelude::*;
```

## Exported Items

### Convenience Functions

#### Reading

```rust
/// Read first RGBA layer with custom storage
pub fn read_first_rgba_layer_from_file<P, C, S, Px>(
    path: impl AsRef<Path>,
    constructor: C,
    setter: S,
) -> Result<Image<Layer<SpecificChannels<P, RgbaChannels>>>>
where
    C: FnOnce(Vec2<usize>, &RgbaChannels) -> P,
    S: FnMut(&mut P, Vec2<usize>, Px);

/// Read all RGBA layers
pub fn read_all_rgba_layers_from_file<...>(...) -> Result<Image<Layers<...>>>;

/// Read first flat layer (any channels)
pub fn read_first_flat_layer_from_file(
    path: impl AsRef<Path>
) -> Result<Image<Layer<AnyChannels<FlatSamples>>>>;

/// Read all flat layers
pub fn read_all_flat_layers_from_file(
    path: impl AsRef<Path>
) -> Result<Image<Layers<AnyChannels<FlatSamples>>>>;

/// Read everything (most flexible)
pub fn read_all_data_from_file(
    path: impl AsRef<Path>
) -> Result<Image<Layers<AnyChannels<FlatSamples>>>>;

/// Read first layer (deep or flat)
pub fn read_first_any_layer_from_file(
    path: impl AsRef<Path>
) -> Result<Image<Layer<AnyChannels<DeepAndFlatSamples>>>>;
```

#### Writing

```rust
/// Write RGBA image
pub fn write_rgba_file<P>(
    path: impl AsRef<Path>,
    width: usize,
    height: usize,
    pixels: impl Fn(usize, usize) -> P,
) -> UnitResult
where P: Into<(impl Into<Sample>, impl Into<Sample>, impl Into<Sample>, impl Into<Sample>)>;

/// Write RGB image
pub fn write_rgb_file<P>(
    path: impl AsRef<Path>,
    width: usize,
    height: usize,
    pixels: impl Fn(usize, usize) -> P,
) -> UnitResult
where P: Into<(impl Into<Sample>, impl Into<Sample>, impl Into<Sample>)>;
```

### Builder Entry Point

```rust
/// Start building a read operation
pub fn read() -> ReadBuilder;
```

### Image Types

```rust
/// Top-level image container
pub struct Image<Layers> {
    pub attributes: ImageAttributes,
    pub layer_data: Layers,
}

/// Single layer
pub struct Layer<Channels> {
    pub channel_data: Channels,
    pub attributes: LayerAttributes,
    pub size: Vec2<usize>,
    pub encoding: Encoding,
}

/// How pixels are stored in file
pub struct Encoding {
    pub compression: Compression,
    pub blocks: Blocks,
    pub line_order: LineOrder,
}
```

### Channel Types

```rust
/// Dynamic channel list
pub struct AnyChannels<Samples> {
    pub list: SmallVec<[AnyChannel<Samples>; 4]>,
}

/// Single channel
pub struct AnyChannel<Samples> {
    pub name: Text,
    pub sample_data: Samples,
    pub quantize_linearly: bool,
    pub sampling: Vec2<usize>,
}

/// Static channel set
pub struct SpecificChannels<Pixels, ChannelDescriptions> {
    pub channels: ChannelDescriptions,
    pub pixels: Pixels,
}
```

### Sample Types

```rust
/// Flat samples (one per pixel)
pub enum FlatSamples {
    F16(Vec<f16>),
    F32(Vec<f32>),
    U32(Vec<u32>),
}

/// Dynamic sample type
pub enum Sample {
    F16(f16),
    F32(f32),
    U32(u32),
}
```

### Metadata Types

```rust
/// Image-level attributes
pub struct ImageAttributes {
    pub display_window: IntegerBounds,
    pub pixel_aspect: f32,
    pub chromaticities: Option<Chromaticities>,
    pub time_code: Option<TimeCode>,
    pub other: HashMap<Text, AttributeValue>,
}

/// Layer-level attributes
pub struct LayerAttributes {
    pub layer_name: Option<Text>,
    pub owner: Option<Text>,
    pub comments: Option<Text>,
    pub software_name: Option<Text>,
    // ... many more
    pub other: HashMap<Text, AttributeValue>,
}

/// Channel description
pub struct ChannelDescription {
    pub name: Text,
    pub sample_type: SampleType,
    pub quantize_linearly: bool,
    pub sampling: Vec2<usize>,
}
```

### Compression

```rust
pub enum Compression {
    Uncompressed,
    RLE,
    ZIPS,
    ZIP,
    PIZ,
    PXR24,
    B44,
    B44A,
    DWAA,  // Not yet implemented
    DWAB,  // Not yet implemented
}
```

### Enums

```rust
pub enum LineOrder {
    Increasing,
    Decreasing,
    Unspecified,
}

pub enum SampleType {
    U32,
    F16,
    F32,
}

pub enum Blocks {
    ScanLines,
    Tiles(TileDescription),
}
```

### Math Types

```rust
/// 2D vector
pub struct Vec2<T>(pub T, pub T);

impl<T> Vec2<T> {
    pub fn x(&self) -> T;
    pub fn y(&self) -> T;
    pub fn width(&self) -> T;   // Alias
    pub fn height(&self) -> T;  // Alias
}

/// Integer rectangle
pub struct IntegerBounds {
    pub position: Vec2<i32>,
    pub size: Vec2<usize>,
}
```

### Error Handling

```rust
pub enum Error {
    NotSupported(Cow<'static, str>),
    Invalid(Cow<'static, str>),
    Io(std::io::Error),
    Aborted,
}

pub type Result<T> = std::result::Result<T, Error>;
pub type UnitResult = Result<()>;
```

### Re-exports

```rust
pub use half::f16;
pub use smallvec::SmallVec;
```

### Traits

```rust
/// Reading traits
pub use crate::image::read::{
    ReadImage,
    ReadLayers,
    ReadChannels,
    ReadSamples,
    ReadSpecificChannel,
};

/// Writing traits
pub use crate::image::write::{
    WritableImage,
    GetPixel,
};

/// Cropping traits
pub use crate::image::crop::{
    Crop,
    CropResult,
    CropWhere,
    ApplyCroppedView,
    CroppedChannels,
    InspectSample,
};
```

## Example

```rust
use exr::prelude::*;

fn main() -> Result<()> {
    // Write
    write_rgba_file("test.exr", 100, 100, |x, y| {
        (x as f32 / 100.0, y as f32 / 100.0, 0.5_f32, 1.0_f32)
    })?;
    
    // Read
    let image = read_all_data_from_file("test.exr")?;
    
    println!("Layers: {}", image.layer_data.len());
    for layer in &image.layer_data {
        println!("  Size: {:?}", layer.size);
        println!("  Channels: {}", layer.channel_data.list.len());
    }
    
    Ok(())
}
```

## See Also

- [Image Module](./image.md) - Full image API
- [Meta Module](./meta.md) - Metadata types
- [User Guide: Quick Start](../user-guide/quick-start.md)
