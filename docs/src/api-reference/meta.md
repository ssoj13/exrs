# Meta Module

The `exr::meta` module contains metadata types for EXR files.

## MetaData

Complete file metadata:

```rust
pub struct MetaData {
    /// File requirements and version
    pub requirements: Requirements,
    
    /// Headers (one per layer)
    pub headers: Headers,
}
```

## Requirements

File format requirements:

```rust
pub struct Requirements {
    /// Format version (usually 2)
    pub file_format_version: u8,
    
    /// Single-part file
    pub is_single_part: bool,
    
    /// Contains deep data
    pub has_deep_data: bool,
    
    /// Has long channel/attribute names
    pub has_long_names: bool,
}
```

## Header

Per-layer header:

```rust
pub struct Header {
    /// Channel definitions
    pub channels: ChannelList,
    
    /// Compression method
    pub compression: Compression,
    
    /// Pixel data dimensions
    pub data_size: Vec2<usize>,
    
    /// Block organization
    pub blocks: BlockDescription,
    
    /// Layer attributes
    pub layer_attributes: LayerAttributes,
    
    /// Shared image attributes (Arc for multipart)
    pub shared_attributes: Option<Arc<ImageAttributes>>,
    
    /// Deep data flag
    pub deep: bool,
    
    /// Deep data type
    pub deep_data_version: Option<i32>,
    
    /// Maximum samples per pixel (deep)
    pub max_samples_per_pixel: Option<usize>,
}
```

## ImageAttributes

Image-level metadata:

```rust
pub struct ImageAttributes {
    /// Display bounds (full frame)
    pub display_window: IntegerBounds,
    
    /// Pixel aspect ratio
    pub pixel_aspect: f32,
    
    /// Color space primaries
    pub chromaticities: Option<Chromaticities>,
    
    /// SMPTE timecode
    pub time_code: Option<TimeCode>,
    
    /// Custom attributes
    pub other: HashMap<Text, AttributeValue>,
}

impl ImageAttributes {
    pub fn new(display_window: IntegerBounds) -> Self;
    
    pub fn with_size(size: impl Into<Vec2<usize>>) -> Self;
}
```

## LayerAttributes

Layer-level metadata:

```rust
pub struct LayerAttributes {
    /// Layer name (required for multipart)
    pub layer_name: Option<Text>,
    
    /// Layer position offset
    pub layer_position: Vec2<i32>,
    
    /// Screen window center
    pub screen_window_center: Vec2<f32>,
    
    /// Screen window width
    pub screen_window_width: f32,
    
    /// White luminance (cd/m^2)
    pub white_luminance: Option<f32>,
    
    /// Adopted neutral
    pub adopted_neutral: Option<Vec2<f32>>,
    
    /// Rendering transform
    pub rendering_transform_name: Option<Text>,
    
    /// Look modification transform
    pub look_modification_transform_name: Option<Text>,
    
    /// X density (pixels per inch)
    pub horizontal_density: Option<f32>,
    
    /// Copyright owner
    pub owner: Option<Text>,
    
    /// Comments
    pub comments: Option<Text>,
    
    /// Capture date
    pub capture_date: Option<Text>,
    
    /// UTC offset
    pub utc_offset: Option<f32>,
    
    /// Longitude
    pub longitude: Option<f32>,
    
    /// Latitude
    pub latitude: Option<f32>,
    
    /// Altitude
    pub altitude: Option<f32>,
    
    /// Focus distance (meters)
    pub focus: Option<f32>,
    
    /// Exposure time (seconds)
    pub exposure: Option<f32>,
    
    /// Aperture
    pub aperture: Option<f32>,
    
    /// ISO speed
    pub iso_speed: Option<f32>,
    
    /// Environment map type
    pub environment_map: Option<EnvironmentMap>,
    
    /// Key code (film)
    pub key_code: Option<KeyCode>,
    
    /// Wrap modes
    pub wrap_mode_name: Option<Text>,
    
    /// Frames per second
    pub frames_per_second: Option<Rational>,
    
    /// Multi-view name
    pub multi_view: Option<Text>,
    
    /// World-to-camera matrix
    pub world_to_camera: Option<Matrix4x4>,
    
    /// World-to-NDC matrix
    pub world_to_normalized_device: Option<Matrix4x4>,
    
    /// Deep image state
    pub deep_image_state: Option<f32>,
    
    /// Original data window
    pub original_data_window: Option<IntegerBounds>,
    
    /// DWA compression level
    pub dwa_compression_level: Option<f32>,
    
    /// Preview image
    pub preview: Option<Preview>,
    
    /// View name
    pub view: Option<Text>,
    
    /// Software name
    pub software_name: Option<Text>,
    
    /// Near clip
    pub near_clip_plane: Option<f32>,
    
    /// Far clip
    pub far_clip_plane: Option<f32>,
    
    /// Field of view (horizontal)
    pub horizontal_field_of_view: Option<f32>,
    
    /// Field of view (vertical)
    pub vertical_field_of_view: Option<f32>,
    
    /// Custom attributes
    pub other: HashMap<Text, AttributeValue>,
}

impl LayerAttributes {
    /// Create with name
    pub fn named(name: impl Into<Text>) -> Self;
    
    /// Create with default values
    pub fn new(name: impl Into<Text>) -> Self;
}
```

## ChannelDescription

Single channel definition:

```rust
pub struct ChannelDescription {
    /// Channel name
    pub name: Text,
    
    /// Sample type
    pub sample_type: SampleType,
    
    /// Linear quantization flag
    pub quantize_linearly: bool,
    
    /// Subsampling rate
    pub sampling: Vec2<usize>,
}

impl ChannelDescription {
    pub fn new(name: impl Into<Text>, sample_type: SampleType) -> Self;
}
```

## SampleType

Pixel sample types:

```rust
pub enum SampleType {
    U32,
    F16,
    F32,
}

impl SampleType {
    pub fn bytes_per_sample(self) -> usize;
}
```

## Compression

Compression methods:

```rust
pub enum Compression {
    /// No compression
    Uncompressed,
    
    /// Run-length encoding
    RLE,
    
    /// ZIP (single scanline)
    ZIPS,
    
    /// ZIP (16 scanlines)
    ZIP,
    
    /// Wavelet (32 scanlines)
    PIZ,
    
    /// 24-bit float (16 scanlines)
    PXR24,
    
    /// Block compression
    B44,
    
    /// Block compression (sparse)
    B44A,
    
    /// DCT (32 scanlines) - not implemented
    DWAA,
    
    /// DCT (256 scanlines) - not implemented
    DWAB,
}

impl Compression {
    /// Scanlines per block
    pub fn scan_lines_per_block(self) -> usize;
    
    /// Is lossless
    pub fn is_lossless(self) -> bool;
    
    /// Supports deep data
    pub fn supports_deep_data(self) -> bool;
}
```

## LineOrder

Scanline ordering:

```rust
pub enum LineOrder {
    /// Top to bottom
    Increasing,
    
    /// Bottom to top
    Decreasing,
    
    /// Any order
    Unspecified,
}
```

## TileDescription

Tile configuration:

```rust
pub struct TileDescription {
    /// Tile size
    pub tile_size: Vec2<usize>,
    
    /// Level mode
    pub level_mode: LevelMode,
    
    /// Rounding mode
    pub rounding_mode: RoundingMode,
}

pub enum LevelMode {
    Singular,
    MipMap,
    RipMap,
}

pub enum RoundingMode {
    Up,
    Down,
}
```

## IntegerBounds

Rectangle with position:

```rust
pub struct IntegerBounds {
    /// Top-left position
    pub position: Vec2<i32>,
    
    /// Size
    pub size: Vec2<usize>,
}

impl IntegerBounds {
    pub fn from_dimensions(size: impl Into<Vec2<usize>>) -> Self;
    
    pub fn zero_min_with_size(size: impl Into<Vec2<usize>>) -> Self;
    
    pub fn end(&self) -> Vec2<i32>;
    
    pub fn contains(&self, position: Vec2<i32>) -> bool;
}
```

## Text

UTF-8 string optimized for small names:

```rust
pub struct Text(SmallVec<[u8; 24]>);

impl Text {
    pub fn from_str_unchecked(text: &str) -> Self;
    pub fn new_or_none(text: &str) -> Option<Self>;
    pub fn new_or_panic(text: &str) -> Self;
    pub fn as_str(&self) -> &str;
    pub fn is_empty(&self) -> bool;
}

impl From<&str> for Text { ... }
impl From<String> for Text { ... }
```

## AttributeValue

Attribute value types:

```rust
pub enum AttributeValue {
    I32(i32),
    F32(f32),
    F64(f64),
    Rational(Rational),
    
    Text(Text),
    TextVector(Vec<Text>),
    
    IntVec2(Vec2<i32>),
    FloatVec2(Vec2<f32>),
    IntVec3((i32, i32, i32)),
    FloatVec3((f32, f32, f32)),
    
    IntRect(IntegerBounds),
    FloatRect(FloatRect),
    
    ChannelList(ChannelList),
    
    Chromaticities(Chromaticities),
    Compression(Compression),
    EnvironmentMap(EnvironmentMap),
    KeyCode(KeyCode),
    LineOrder(LineOrder),
    Matrix3x3([f32; 9]),
    Matrix4x4([f32; 16]),
    Preview(Preview),
    TileDescription(TileDescription),
    TimeCode(TimeCode),
    
    /// Custom/unknown attribute
    Custom {
        type_name: Text,
        bytes: Vec<u8>,
    },
}
```

## Chromaticities

Color space definition:

```rust
pub struct Chromaticities {
    pub red: Vec2<f32>,
    pub green: Vec2<f32>,
    pub blue: Vec2<f32>,
    pub white: Vec2<f32>,
}

impl Chromaticities {
    /// sRGB/Rec.709
    pub const SRGB: Chromaticities;
    
    /// Adobe RGB
    pub const ADOBE_RGB: Chromaticities;
    
    /// DCI-P3
    pub const DCI_P3: Chromaticities;
}
```

## TimeCode

SMPTE timecode:

```rust
pub struct TimeCode {
    pub hours: u8,
    pub minutes: u8,
    pub seconds: u8,
    pub frame: u8,
    pub drop_frame: bool,
    pub color_frame: bool,
    pub field_phase: bool,
    pub binary_group_flags: [bool; 3],
    pub binary_groups: [u8; 8],
}
```

## Preview

Thumbnail image:

```rust
pub struct Preview {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<(u8, u8, u8, u8)>,  // RGBA
}
```

## See Also

- [Prelude](./prelude.md) - Common exports
- [Image Module](./image.md) - High-level API
