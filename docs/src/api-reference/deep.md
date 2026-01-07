# Deep Data Module

API reference for deep data (OpenEXR 2.0) support.

## Core Types

### DeepSamples

Variable-length sample storage per pixel:

```rust
pub enum DeepSamples<T> {
    /// Flat data (1 sample per pixel)
    Flat(Vec<T>),
    
    /// Deep data (variable samples per pixel)
    Deep {
        /// Sample counts per pixel
        sample_counts: Vec<u32>,
        /// All samples flattened
        samples: Vec<T>,
    },
}
```

### DeepSamples Methods

```rust
impl<T> DeepSamples<T> {
    /// Create deep samples from counts and data
    pub fn deep(sample_counts: Vec<u32>, samples: Vec<T>) -> Self;
    
    /// Create flat samples (1 per pixel)
    pub fn flat(samples: Vec<T>) -> Self;
    
    /// Check if this is deep data
    pub fn is_deep(&self) -> bool;
    
    /// Total number of samples across all pixels
    pub fn total_sample_count(&self) -> usize;
    
    /// Get sample count for a specific pixel
    pub fn sample_count_at(&self, pixel_index: usize) -> u32;
    
    /// Get samples for a specific pixel
    pub fn samples_at(&self, pixel_index: usize) -> &[T];
    
    /// Iterate over all pixels with their samples
    pub fn iter_pixels(&self) -> impl Iterator<Item = &[T]>;
}
```

## Reading Deep Data

### Using UnifiedReader

```rust
use exrs::prelude::*;

// Read any EXR (flat or deep) with unified API
let image = read()
    .no_deep_data()  // Ignore deep, read flat only
    .all_resolution_levels()
    .all_channels()
    .all_layers()
    .all_attributes()
    .from_file("image.exr")?;
```

### Reading Deep Channels

```rust
use exrs::prelude::*;
use exrs::image::DeepSamples;

// Custom channel reading with deep support
let image = read()
    .all_resolution_levels()
    .all_channels()
    .all_layers()
    .all_attributes()
    .from_file("deep.exr")?;

// Access deep samples
for layer in &image.layer_data {
    match &layer.channel_data {
        AnyChannels::Deep(channels) => {
            for channel in channels {
                println!("Channel: {}", channel.name);
                // Process deep samples...
            }
        }
        AnyChannels::Flat(channels) => {
            // Handle flat data
        }
    }
}
```

## Writing Deep Data

### Creating Deep Images

```rust
use exrs::prelude::*;
use exrs::image::DeepSamples;

// Create sample counts (variable per pixel)
let width = 1920;
let height = 1080;
let mut sample_counts = Vec::with_capacity(width * height);
let mut samples = Vec::new();

for y in 0..height {
    for x in 0..width {
        // Varying sample count per pixel
        let count = if x % 10 == 0 { 3 } else { 1 };
        sample_counts.push(count as u32);
        
        for i in 0..count {
            samples.push(0.5f32 + i as f32 * 0.1);
        }
    }
}

let deep_samples = DeepSamples::deep(sample_counts, samples);
```

## Deep Data Structure

### Pixel Layout

```
Pixel (0,0): [sample0, sample1, sample2]  // 3 samples
Pixel (1,0): [sample0]                     // 1 sample
Pixel (2,0): [sample0, sample1]            // 2 samples
...

sample_counts: [3, 1, 2, ...]
samples: [s0_0, s0_1, s0_2, s1_0, s2_0, s2_1, ...]
```

### Memory Layout

```rust
// Accessing pixel samples
let pixel_index = y * width + x;
let start = sample_counts[..pixel_index].iter().sum::<u32>() as usize;
let count = sample_counts[pixel_index] as usize;
let pixel_samples = &samples[start..start + count];
```

## Block Processing

### DeepTileBlock

```rust
pub struct DeepTileBlock {
    /// Tile coordinates
    pub coordinates: TileCoordinates,
    /// Sample counts per pixel in tile
    pub sample_counts: Vec<u32>,
    /// Compressed sample data
    pub compressed_samples: Vec<u8>,
}
```

### DeepScanLineBlock

```rust
pub struct DeepScanLineBlock {
    /// Starting Y coordinate
    pub y_coordinate: i32,
    /// Sample counts for scanline range
    pub sample_counts: Vec<u32>,
    /// Compressed sample data
    pub compressed_samples: Vec<u8>,
}
```

## Compression for Deep Data

Supported compression methods for deep data:

```rust
// These work with deep data:
Compression::Uncompressed  // No compression
Compression::RLE           // Run-length encoding
Compression::ZIP1          // ZIP, 1 scanline
Compression::ZIP16         // ZIP, 16 scanlines (recommended)

// These do NOT support deep data:
Compression::PIZ           // Wavelet - flat only
Compression::PXR24         // Lossy - flat only
Compression::B44           // Fixed rate - flat only
Compression::DWAA          // DCT - flat only
```

## Parallel Processing

Deep data benefits from parallel decompression:

```rust
// Automatic parallel processing with rayon
// Each block is decompressed independently

// Performance comparison (4K deep image):
// Sequential: ~2.5s
// Parallel (8 cores): ~1.0s
// Speedup: 2.5x
```

## Use Cases

### Volumetric Data

```rust
// Store multiple density samples per pixel
// Front-to-back depth ordering
DeepSamples::deep(
    vec![5, 3, 8, ...],  // Variable samples per ray
    vec![/* density values at each depth */]
)
```

### Particle Rendering

```rust
// Store particle contributions per pixel
// Each particle adds a sample at its depth
DeepSamples::deep(
    sample_counts,  // Particles visible per pixel
    particle_colors // RGBA per particle
)
```

### Deep Compositing

```rust
// Non-destructive compositing with depth
// Merge deep images by combining samples
// Sort by depth, composite front-to-back
```

## Best Practices

1. **Use ZIP16** for deep data compression (best balance)
2. **Pre-allocate** sample vectors when counts are known
3. **Sort samples** by depth for proper compositing
4. **Use parallel reading** for large deep images
5. **Validate sample counts** match expected pixel count

## Error Handling

```rust
use exrs::error::Error;

match read().from_file("deep.exr") {
    Ok(image) => { /* process */ }
    Err(Error::Invalid(msg)) => {
        eprintln!("Invalid deep data: {}", msg);
    }
    Err(e) => {
        eprintln!("Read error: {}", e);
    }
}
```
