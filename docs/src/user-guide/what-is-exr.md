# What is OpenEXR?

**OpenEXR** is an open-source, high dynamic range (HDR) image file format developed by Industrial Light & Magic (ILM) for use in computer graphics applications.

## History

OpenEXR was originally developed at ILM in the late 1990s and was released as open source in 2003. Since then, it has become the industry standard for visual effects, animation, and film production.

Key milestones:
- **1999** - Internal development at ILM
- **2003** - Open source release (v1.0)
- **2013** - OpenEXR 2.0 with deep data support
- **2020** - Moved to Academy Software Foundation (ASWF)

## Format Overview

An OpenEXR file consists of:

```
+---------------------------+
|      Magic Number         |  4 bytes: 0x762f3101
+---------------------------+
|    Version + Flags        |  4 bytes
+---------------------------+
|         Headers           |  Attributes as key-value pairs
+---------------------------+
|      Offset Tables        |  Pointers to pixel data chunks
+---------------------------+
|       Pixel Data          |  Compressed/uncompressed chunks
+---------------------------+
```

### File Types

| Type | Description | Use Case |
|------|-------------|----------|
| **Scanline** | Row-by-row storage | General images |
| **Tiled** | Block-based storage | Large images, streaming |
| **Multi-part** | Multiple independent images | Layered compositing |
| **Deep** | Variable samples per pixel | Volumetrics, particles |

## Why "EXR"?

The name comes from the file extension `.exr`. The library was named "IlmImf" internally (ILM Image Format), but the format became known by its extension.

## Key Technical Features

### High Dynamic Range

Unlike 8-bit formats (PNG, JPEG), EXR stores actual light values:

```
8-bit:  0-255 (256 values)
16-bit float: 6x10^-8 to 6.5x10^4 (dynamic range ~10^9)
32-bit float: Full IEEE 754 range
```

This preserves:
- Highlights that would clip in LDR formats
- Shadow detail
- Physical light ratios for accurate compositing

### Pixel Types

| Type | Size | Range | Precision |
|------|------|-------|-----------|
| `half` (f16) | 16-bit | ~6e-8 to 65504 | 3 decimal digits |
| `float` (f32) | 32-bit | Full IEEE 754 | 7 decimal digits |
| `uint` (u32) | 32-bit | 0 to 4,294,967,295 | Exact integers |

### Arbitrary Channels

Unlike fixed RGB/RGBA formats, EXR supports any channels:

```
Standard:    R, G, B, A
Extended:    R, G, B, A, Z (depth), N.x, N.y, N.z (normals)
VFX:         R, G, B, A, Z, ZBack, motion.u, motion.v, crypto_object
Custom:      Any name you need
```

### Compression Methods

| Method | Type | Best For |
|--------|------|----------|
| None | Uncompressed | Maximum speed |
| RLE | Lossless | Simple patterns |
| ZIP | Lossless | General use |
| ZIPS | Lossless | Single scanlines |
| PIZ | Lossless | Noisy images |
| PXR24 | Lossy (f32→f24) | Large float images |
| B44/B44A | Lossy | Real-time playback |
| DWAA/DWAB | Lossy | Smallest files |

### Layers (Multi-Part)

EXR can contain multiple independent images:

```
my_render.exr
├── beauty (main RGBA)
├── diffuse (RGB)
├── specular (RGB)
├── reflection (RGB)
├── shadow (A)
├── depth (Z)
└── motion (UV)
```

### Resolution Levels

**Mip Maps** - Power-of-2 downscaled versions:
```
Level 0: 4096 x 2048
Level 1: 2048 x 1024
Level 2: 1024 x 512
...
```

**Rip Maps** - Independent X/Y scaling:
```
Level (0,0): 4096 x 2048
Level (1,0): 2048 x 2048
Level (0,1): 4096 x 1024
Level (1,1): 2048 x 1024
...
```

## Deep Data (OpenEXR 2.0)

Traditional (flat) images store one sample per pixel. Deep images store **variable samples per pixel**:

```
Flat image:
  Pixel (10, 20) = RGBA(0.5, 0.3, 0.1, 1.0)

Deep image:
  Pixel (10, 20) = [
    {Z=1.5, RGBA(0.2, 0.1, 0.0, 0.3)},  // Smoke at depth 1.5
    {Z=5.0, RGBA(0.8, 0.6, 0.4, 1.0)},  // Object at depth 5.0
    {Z=8.2, RGBA(0.1, 0.1, 0.1, 0.1)},  // More smoke at depth 8.2
  ]
```

Use cases:
- Volumetric rendering (smoke, fog, clouds)
- Particle systems
- Hair/fur with transparency
- Deep compositing without artifacts

## Metadata (Attributes)

EXR supports extensive metadata:

| Attribute | Type | Description |
|-----------|------|-------------|
| `displayWindow` | Box2i | Full frame dimensions |
| `dataWindow` | Box2i | Actual pixel bounds |
| `pixelAspectRatio` | float | Pixel shape |
| `chromaticities` | Chromaticities | Color space primaries |
| `whiteLuminance` | float | Absolute brightness |
| `timeCode` | TimeCode | SMPTE timecode |
| `framesPerSecond` | Rational | Frame rate |
| `owner` | string | Copyright holder |
| Custom | Any | Your own attributes |

## Industry Usage

OpenEXR is used in virtually every major VFX and animation production:

- **Film**: Avatar, Avengers, Star Wars, Marvel films
- **Animation**: Pixar, DreamWorks, Disney, Sony Animation
- **Games**: Asset pipelines, HDR textures
- **Software**: Nuke, Maya, Houdini, Blender, After Effects, DaVinci Resolve

## Comparison with Other Formats

| Feature | EXR | PNG | TIFF | HDR |
|---------|-----|-----|------|-----|
| HDR | Yes | No | Limited | Yes |
| Bit Depth | 16/32 float | 8/16 int | 8/16/32 | 32 float |
| Layers | Yes | No | Yes | No |
| Deep Data | Yes | No | No | No |
| Compression | Multiple | Lossless | Multiple | RLE |
| Metadata | Extensive | Limited | Good | Minimal |
| Color Spaces | Any | sRGB | Various | Linear |

## Next Steps

- [Why Use EXR?](./why-exr.md) - Benefits for your workflow
- [Quick Start](./quick-start.md) - Start reading and writing EXR files
