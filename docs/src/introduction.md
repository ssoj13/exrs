# EXRS - Rust OpenEXR Library

**exrs** is a 100% Rust, 100% safe code library for reading and writing OpenEXR images.

[![Crates.io](https://img.shields.io/crates/v/exr.svg)](https://crates.io/crates/exr)
[![Docs.rs](https://docs.rs/exr/badge.svg)](https://docs.rs/exr)

## What is this library?

This library provides a pure Rust implementation of the OpenEXR image format, the de-facto standard in animation, VFX, and computer graphics pipelines. Unlike bindings to the C++ reference implementation, exrs:

- Uses **no unsafe code** (`#[forbid(unsafe_code)]`)
- Requires **no CMake** or external dependencies
- Works on **WebAssembly** out of the box
- Provides a **modern, ergonomic Rust API**

## Key Features

- **Multi-layer images** - Any number of layers placed anywhere in 2D space
- **Flexible channels** - RGB, RGBA, XYZ, LAB, depth, motion, masks - anything
- **HDR precision** - 16-bit float, 32-bit float, or 32-bit unsigned integer per channel
- **Deep data** - Variable samples per pixel for volumetric effects (OpenEXR 2.0)
- **Compression** - Lossless (ZIP, RLE, PIZ, B44) and lossy options
- **Multi-resolution** - Mip maps and rip maps support
- **Parallel processing** - Compress/decompress on multiple threads
- **Custom metadata** - Arbitrary attributes with full backwards compatibility

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
exr = "1.74.0"
```

For optimal performance:

```toml
[profile.release]
lto = true
```

## Quick Example

```rust
use exr::prelude::*;

fn main() {
    // Write a simple RGBA image
    write_rgba_file(
        "output.exr",
        1920, 1080,
        |x, y| {
            let r = x as f32 / 1920.0;
            let g = y as f32 / 1080.0;
            let b = 0.5;
            let a = 1.0;
            (r, g, b, a)
        }
    ).unwrap();
    
    // Read it back
    let image = read_first_rgba_layer_from_file(
        "output.exr",
        |resolution, _| vec![(0.0f32, 0.0, 0.0, 0.0); resolution.width() * resolution.height()],
        |pixels, pos, (r, g, b, a): (f32, f32, f32, f32)| {
            pixels[pos.y() * pos.width() + pos.x()] = (r, g, b, a);
        }
    ).unwrap();
}
```

## Documentation Structure

This documentation is organized into three main sections:

1. **[User Guide](./user-guide/what-is-exr.md)** - Learn about EXR format, basic usage, and our enhancements
2. **[Developer Guide](./developer-guide/architecture.md)** - Understand the library architecture and contribute
3. **[API Reference](./api-reference/prelude.md)** - Detailed module and type documentation

## License

BSD-3-Clause

## Contributing

Contributions are welcome! See the [Contributing](./developer-guide/contributing.md) guide.
