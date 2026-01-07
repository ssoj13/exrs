# Our Enhancements

This fork of exrs significantly extends the original library with new features, improved APIs, and comprehensive deep data support.

## Enhancement Summary

| Category | Lines Added | Key Features |
|----------|-------------|--------------|
| **Deep Data** | ~3,500 | Full OpenEXR 2.0 deep support |
| **Unified Reader** | ~800 | Read any file without knowing format |
| **Level Selection** | ~400 | Choose specific mipmap levels |
| **Layer Filtering** | ~300 | Read only matching layers |
| **Compression** | ~1,600 | Deep compression, optimizations |
| **Block Layer** | ~1,100 | Deep block I/O, validation |
| **Metadata** | ~2,300 | Deep headers, attributes |
| **Tests** | ~600 | Deep tests, benchmarks |

**Total: ~25,000 lines added, ~4,000 removed, 82 files modified**

## Major Features

### 1. Deep Data Support (OpenEXR 2.0)

Complete implementation of variable samples per pixel:

```rust
use exr::image::read::deep::read_first_deep_layer_from_file;

let deep = read_first_deep_layer_from_file("particles.exr")?;
let samples = &deep.layer_data.channel_data.list[0].sample_data;

println!("Total samples: {}", samples.total_samples());
for y in 0..samples.height {
    for x in 0..samples.width {
        let count = samples.sample_count(x, y);
        if count > 0 {
            // Process samples
        }
    }
}
```

[Read more about Deep Data Support](./deep-data.md)

### 2. Unified Deep/Flat Reader

Read any EXR file without knowing if it's deep or flat:

```rust
use exr::prelude::*;

let image = read_first_any_layer_from_file("unknown.exr")?;

for channel in &image.layer_data.channel_data.list {
    match &channel.sample_data {
        DeepAndFlatSamples::Deep(deep) => {
            println!("{}: Deep with {} samples", 
                channel.name, deep.total_samples());
        }
        DeepAndFlatSamples::Flat(flat) => {
            println!("{}: Flat with {} samples", 
                channel.name, flat.len());
        }
    }
}
```

### 3. Specific Resolution Level Selection

Load exactly the mipmap level you need:

```rust
use exr::prelude::*;

// Load level 1 (half resolution)
let image = read()
    .no_deep_data()
    .specific_resolution_level(|_| Vec2(1, 1))
    .all_channels()
    .first_valid_layer()
    .from_file("mipmapped.exr")?;

// Load level closest to 512px
let image = read()
    .no_deep_data()
    .specific_resolution_level(|levels| {
        levels.iter()
            .min_by_key(|l| (l.resolution.x() as i64 - 512).abs())
            .map(|l| l.index)
            .unwrap_or(Vec2(0, 0))
    })
    .all_channels()
    .first_valid_layer()
    .from_file("mipmapped.exr")?;
```

### 4. All Valid Layers Reader

Read only layers matching your channel requirements:

```rust
use exr::prelude::*;

let image = read()
    .no_deep_data()
    .largest_resolution_level()
    .rgb_channels(create_pixels, set_pixel)
    .all_valid_layers()  // Skip layers without RGB
    .all_attributes()
    .from_file("mixed_layers.exr")?;

println!("Found {} RGB layers", image.layer_data.len());
```

### 5. Parallel Deep Decompression

Automatic multi-threaded decompression for deep data:

| File | Sequential | Parallel | Speedup |
|------|------------|----------|---------|
| Teaset720p.exr | 199ms | 79ms | **2.52x** |
| Ground.exr | 84ms | 37ms | **2.27x** |
| MiniCooper720p.exr | 150ms | 84ms | **1.79x** |

[Read more about General Improvements](./general.md)

## Breaking Changes

**None** - All changes are additive and backwards compatible with existing code.

## Branch Information

- **Branch:** `deep`
- **Base:** `master` (v1.74.0)
- **Status:** All tests passing

## What's Next

- [Deep Data Support](./deep-data.md) - Detailed deep data documentation
- [General Improvements](./general.md) - Other enhancements and optimizations
