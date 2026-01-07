# Testing

Comprehensive guide to testing exrs.

## Test Categories

### Unit Tests

Located within source files:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_compression() {
        let data = vec![1, 2, 3, 4];
        let compressed = compress(&data).unwrap();
        let decompressed = decompress(&compressed, data.len()).unwrap();
        assert_eq!(data, decompressed);
    }
}
```

### Integration Tests

Located in `tests/`:

| File | Purpose |
|------|---------|
| `roundtrip.rs` | Write then read, verify equality |
| `across_compression.rs` | Test all compression methods |
| `deep_read.rs` | Deep data reading |
| `deep_benchmark.rs` | Deep performance |
| `fuzz.rs` | Fuzz testing |
| `dev.rs` | Development tests |

### Fuzz Tests

Randomized testing for robustness:

```rust
#[test]
#[ignore]  // Run explicitly
fn fuzz() {
    loop {
        let random_image = generate_random_image();
        write_and_read_back(random_image);
    }
}
```

## Running Tests

### All Tests

```bash
cargo test
```

### Specific Test

```bash
cargo test test_name
```

### With Output

```bash
cargo test -- --nocapture
```

### Release Mode

```bash
cargo test --release
```

### Ignored Tests

```bash
cargo test -- --ignored
```

### Fuzz Testing

```bash
# Run indefinitely
cargo test --package exr --test fuzz fuzz -- --exact --ignored
```

## Test Images

### Directory Structure

```
tests/images/
├── valid/
│   ├── custom/              # Custom test images
│   │   ├── compression_methods/
│   │   └── crowskull/
│   └── openexr/             # Official test images
│       ├── Beachball/
│       ├── Chromaticities/
│       ├── DisplayWindow/
│       ├── IlmfmlmflmTest/
│       ├── LuminanceChroma/
│       ├── MultiResolution/
│       ├── MultiView/
│       ├── ScanLines/
│       ├── TestImages/
│       ├── Tiles/
│       └── v2/              # Deep data images
│           ├── deep_large/
│           ├── LeftView/
│           ├── LowResLeftView/
│           └── Stereo/
└── invalid/
    ├── fuzzed/              # Fuzz-discovered crashes
    └── openexr/
        ├── Damaged/         # Intentionally corrupt
        └── IlmlfmlflmTest/  # Edge cases
```

### Deep Test Images

Official OpenEXR deep test files:

| File | Size | Samples | Use |
|------|------|---------|-----|
| MiniCooper720p.exr | ~5MB | 932K | Benchmark |
| PiranhnaAlienRun720p.exr | ~4MB | ~500K | Particles |
| Teaset720p.exr | ~6MB | 997K | Complex scene |
| Balls.exr | ~1MB | 94K | Simple shapes |
| Ground.exr | ~2MB | 360K | Ground plane |

## Writing Tests

### Basic Test

```rust
#[test]
fn test_read_write() {
    use exr::prelude::*;
    
    // Create test image
    let image = Image::from_channels(
        (100, 100),
        SpecificChannels::rgba(|pos| {
            (pos.x() as f32 / 100.0, 0.0, 0.0, 1.0)
        })
    );
    
    // Write
    let mut bytes = Vec::new();
    image.write().to_buffered(Cursor::new(&mut bytes)).unwrap();
    
    // Read back
    let loaded = read_all_data_from_file(...).unwrap();
    
    // Verify
    assert_eq!(image.layer_data.size, loaded.layer_data[0].size);
}
```

### Roundtrip Test

```rust
#[test]
fn roundtrip_compression() {
    for compression in [
        Compression::Uncompressed,
        Compression::RLE,
        Compression::ZIPS,
        Compression::ZIP,
        Compression::PIZ,
        Compression::PXR24,
        Compression::B44,
    ] {
        let layer = Layer::new(
            (256, 256),
            LayerAttributes::named("test"),
            Encoding {
                compression,
                ..Default::default()
            },
            generate_test_channels()
        );
        
        let image = Image::from_layer(layer);
        
        // Write and read
        let mut buffer = Vec::new();
        image.write().to_buffered(Cursor::new(&mut buffer)).unwrap();
        
        let loaded = read_all_data_from_file(Cursor::new(&buffer)).unwrap();
        
        // Verify (accounting for lossy compression)
        verify_similar(&image, &loaded, compression.is_lossless());
    }
}
```

### Deep Data Test

```rust
#[test]
fn test_deep_read() {
    use exr::image::read::deep::read_first_deep_layer_from_file;
    
    let image = read_first_deep_layer_from_file(
        "tests/images/valid/openexr/v2/LowResLeftView/Balls.exr"
    ).expect("failed to read deep image");
    
    let samples = &image.layer_data.channel_data.list[0].sample_data;
    
    // Verify structure
    assert!(samples.total_samples() > 0);
    assert!(samples.width > 0);
    assert!(samples.height > 0);
    
    // Verify data integrity
    for y in 0..samples.height {
        for x in 0..samples.width {
            let count = samples.sample_count(x, y);
            let (start, end) = samples.sample_range(y * samples.width + x);
            assert_eq!(end - start, count);
        }
    }
}
```

### Benchmark Test

```rust
#[test]
fn benchmark_deep_read() {
    use std::time::Instant;
    
    let files = [
        "tests/images/valid/openexr/v2/deep_large/Teaset720p.exr",
    ];
    
    for path in files {
        // Parallel
        let start = Instant::now();
        let _ = read_first_deep_layer_from_file(path);
        let parallel = start.elapsed();
        
        // Sequential
        let start = Instant::now();
        let _ = read_deep()
            .all_channels()
            .first_valid_layer()
            .all_attributes()
            .non_parallel()
            .from_file(path);
        let sequential = start.elapsed();
        
        println!("{}: parallel={:?}, sequential={:?}, speedup={:.2}x",
            path, parallel, sequential,
            sequential.as_secs_f64() / parallel.as_secs_f64()
        );
    }
}
```

## Test Utilities

### Validate Results Module

```rust
#[cfg(any(test, feature = "test-utils"))]
pub mod validate_results {
    pub fn compare_images(a: &Image, b: &Image, tolerance: f32) -> bool;
    pub fn verify_deep_structure(samples: &DeepSamples) -> bool;
}
```

### Test Helpers

```rust
fn generate_test_image(width: usize, height: usize) -> Image<...> {
    Image::from_channels(
        (width, height),
        SpecificChannels::rgba(|pos| {
            let x = pos.x() as f32 / width as f32;
            let y = pos.y() as f32 / height as f32;
            (x, y, (x + y) / 2.0, 1.0)
        })
    )
}

fn temp_file() -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!("exr_test_{}.exr", rand::random::<u64>()));
    path
}
```

## Continuous Integration

### GitHub Actions

```yaml
# .github/workflows/rust.yml
name: Rust

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
    - uses: actions/checkout@v2
    - name: Build
      run: cargo build --verbose
    - name: Run tests
      run: cargo test --verbose
    - name: Clippy
      run: cargo clippy --all-targets
```

### Cross-Platform Testing

```bash
# PowerPC (big-endian)
cross test --target powerpc-unknown-linux-gnu --verbose
```

## Coverage

### Generate Coverage

```bash
cargo install cargo-tarpaulin
cargo tarpaulin --out Html
```

### View Report

Open `tarpaulin-report.html` in browser.

## Performance Testing

### Benchmarks

```bash
cargo bench
```

### Profiling

```bash
cargo build --release
# Use your preferred profiler (perf, Instruments, etc.)
```

### Memory Usage

```bash
cargo build --release
valgrind --tool=massif ./target/release/example
```

## Troubleshooting

### Test Fails with Missing File

Ensure test images are downloaded:
```bash
git lfs pull
```

### Test Hangs

Fuzz test runs forever by design. Use Ctrl+C to stop.

### Memory Issues

Some tests create large images. Use `--test-threads=1`:
```bash
cargo test -- --test-threads=1
```

## See Also

- [Contributing](./contributing.md) - How to contribute
- [Architecture](./architecture.md) - Code structure
