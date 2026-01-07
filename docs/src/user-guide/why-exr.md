# Why Use EXR?

OpenEXR solves real problems in professional graphics pipelines. Here's why you should consider it.

## High Dynamic Range Preservation

### The Problem with 8-bit

Standard 8-bit images (PNG, JPEG) can only represent 256 brightness levels. Real-world scenes have much greater range:

```
Sunlight:     ~100,000 lux
Indoor light: ~500 lux
Moonlight:    ~0.1 lux
```

That's a range of 1,000,000:1 - impossible to capture in 256 values.

### EXR Solution

EXR's floating-point storage preserves the full dynamic range:

```rust
use exr::prelude::*;

// Store actual physical light values
write_rgba_file("hdr_scene.exr", 1920, 1080, |x, y| {
    let sun = 100000.0_f32;      // Direct sunlight
    let sky = 10000.0_f32;       // Sky brightness  
    let shadow = 100.0_f32;      // Deep shadow
    
    // All values preserved exactly
    (sun, sky, shadow, 1.0)
}).unwrap();
```

Benefits:
- **No clipping** - Highlights preserved for later adjustment
- **No banding** - Smooth gradients in shadows
- **Accurate compositing** - Correct light addition/multiplication

## Lossless Workflow

### The Problem

Every save in lossy formats (JPEG) degrades quality. Even "high quality" settings accumulate artifacts through a pipeline.

### EXR Solution

Most EXR compression is lossless:

```rust
use exr::prelude::*;

let layer = Layer::new(
    (1920, 1080),
    LayerAttributes::named("main"),
    Encoding {
        compression: Compression::ZIP16,  // Lossless, good compression
        ..Default::default()
    },
    channels
);
```

Your pixels are mathematically identical after decompression.

## Flexible Channel Support

### The Problem

Fixed formats force you into RGB or RGBA. What about:
- Depth (Z) for depth of field?
- Normals (N.xyz) for relighting?
- Motion vectors (motion.uv) for motion blur?
- Cryptomatte IDs for selection?

### EXR Solution

Add any channels you need:

```rust
use exr::prelude::*;

let channels = AnyChannels::sort(smallvec![
    AnyChannel::new("R", FlatSamples::F16(red_data)),
    AnyChannel::new("G", FlatSamples::F16(green_data)),
    AnyChannel::new("B", FlatSamples::F16(blue_data)),
    AnyChannel::new("A", FlatSamples::F16(alpha_data)),
    AnyChannel::new("Z", FlatSamples::F32(depth_data)),
    AnyChannel::new("N.x", FlatSamples::F16(normal_x)),
    AnyChannel::new("N.y", FlatSamples::F16(normal_y)),
    AnyChannel::new("N.z", FlatSamples::F16(normal_z)),
    AnyChannel::new("motion.u", FlatSamples::F16(motion_u)),
    AnyChannel::new("motion.v", FlatSamples::F16(motion_v)),
]);
```

## Multi-Layer Storage

### The Problem

Render passes typically create dozens of files:
- `render_beauty.png`
- `render_diffuse.png`
- `render_specular.png`
- ...and 20 more

Managing versions, frame numbers, and synchronization is painful.

### EXR Solution

One file, all passes:

```rust
use exr::prelude::*;

let image = Image::empty(ImageAttributes::new(IntegerBounds::from_dimensions((1920, 1080))))
    .with_layer(beauty_layer)
    .with_layer(diffuse_layer)
    .with_layer(specular_layer)
    .with_layer(depth_layer);

image.write().to_file("render_0001.exr").unwrap();
```

Benefits:
- Atomic saves - all passes or none
- Single version number
- Guaranteed synchronization
- Easier file management

## Arbitrary Metadata

### The Problem

You need to store:
- Camera settings (focal length, f-stop)
- Render settings (samples, time)
- Color space information
- Custom production data

Most formats have limited or no metadata support.

### EXR Solution

EXR supports extensive built-in and custom attributes:

```rust
use exr::prelude::*;

let mut attrs = LayerAttributes::named("main");

// Built-in attributes
attrs.owner = Some(Text::from("Studio XYZ"));
attrs.comments = Some(Text::from("Final render v3"));
attrs.software_name = Some(Text::from("Custom Renderer 2.0"));

// Custom attributes
attrs.other.insert(
    Text::from("renderTime"),
    AttributeValue::F32(3600.5)  // 1 hour render
);
attrs.other.insert(
    Text::from("samples"),
    AttributeValue::I32(4096)
);
```

## Efficient Compression

### Compression Comparison

For a typical VFX frame (1920x1080, RGBA float):

| Format | Compression | Size | Quality |
|--------|-------------|------|---------|
| Uncompressed | None | 32 MB | Perfect |
| EXR ZIP | Lossless | 8-15 MB | Perfect |
| EXR PIZ | Lossless | 6-12 MB | Perfect |
| EXR B44 | Lossy | 4-8 MB | Excellent |
| PNG 16-bit | Lossless | 12-20 MB | LDR only |
| JPEG | Lossy | 0.5-2 MB | Artifacts |

### Choosing Compression

```rust
use exr::prelude::*;

// Fast read/write, larger files
Compression::Uncompressed

// Good balance (recommended default)
Compression::ZIP16

// Best for noisy/film grain images
Compression::PIZ

// Fastest compression, single scanlines
Compression::ZIPS

// Lossy but fast decompression (playback)
Compression::B44
```

## Deep Data for VFX

### The Problem

Traditional compositing with pre-multiplied alpha causes artifacts:
- Edge fringing when backgrounds change
- Incorrect transparency stacking
- Loss of depth information

### EXR Deep Data Solution

Store full depth information per pixel:

```rust
use exr::image::read::deep::read_first_deep_layer_from_file;

let deep_image = read_first_deep_layer_from_file("particles.exr").unwrap();

// Each pixel can have 0 to thousands of samples
for y in 0..height {
    for x in 0..width {
        let count = samples.sample_count(x, y);
        // Process each depth sample
    }
}
```

Benefits:
- Correct transparency at any depth
- Re-composite with different backgrounds
- Merge volumetric elements properly

## Industry Compatibility

### Supported Software

EXR is read/written by virtually all professional tools:

**Compositing:**
- Nuke
- After Effects
- Fusion
- Natron

**3D/Rendering:**
- Maya
- Houdini
- Blender
- 3ds Max
- Cinema 4D
- Arnold, V-Ray, RenderMan, etc.

**Image Editing:**
- Photoshop
- GIMP
- Krita

**Color Grading:**
- DaVinci Resolve
- Baselight

### Pipeline Integration

EXR is often the *only* format that can pass through an entire pipeline without data loss:

```
Render (HDR) → Composite → Color Grade → Final Output
     ↓              ↓            ↓
    EXR            EXR          EXR → Delivery format
```

## Why exrs Specifically?

This Rust library offers advantages over C++ bindings:

| Feature | exrs | C++ Bindings |
|---------|------|--------------|
| Safety | `#[forbid(unsafe_code)]` | Inherits C++ issues |
| Build | `cargo add exr` | CMake, env vars |
| WASM | Works | Difficult |
| API | Modern Rust idioms | C++ style |
| Dependencies | Pure Rust | libexr, zlib, etc. |

## When NOT to Use EXR

EXR isn't always the right choice:

- **Web delivery** - Use JPEG, WebP, AVIF
- **Simple screenshots** - PNG is simpler
- **Video** - Use video codecs (ProRes, DNxHR)
- **Texture streaming** - Use GPU-native formats (BC, ASTC)

## Summary

Use EXR when you need:
- HDR data preservation
- Lossless quality
- Multiple channels/layers
- Rich metadata
- VFX pipeline compatibility
- Deep compositing

## Next Steps

- [Quick Start](./quick-start.md) - Begin using exrs
- [Reading Images](./reading.md) - Load EXR files
- [Writing Images](./writing.md) - Create EXR files
