# General Improvements

Beyond deep data, we've made numerous improvements to the exrs library.

## New API Features

### Specific Resolution Level Selection

Load exactly the mipmap level you need instead of all or largest only:

```rust
use exr::prelude::*;

// New: Select specific level
let image = read()
    .no_deep_data()
    .specific_resolution_level(|levels: Vec<LevelInfo>| {
        // levels contains: index, resolution for each level
        // Return the level index you want
        Vec2(1, 1)  // Level 1 in both X and Y
    })
    .all_channels()
    .first_valid_layer()
    .from_file("mipmapped.exr")?;
```

**New types:**
- `LevelInfo { index: Vec2<usize>, resolution: Vec2<usize> }`
- `ReadSpecificLevel<S, F>` - Builder type
- `SpecificLevelReader` - Reader implementation

**Use cases:**
- Thumbnail generation (load small level)
- LOD systems (load appropriate size)
- Memory-constrained environments

### All Valid Layers Reader

Read only layers matching your channel requirements:

```rust
use exr::prelude::*;

// Previously: all_layers() would fail if ANY layer lacked RGB
// Now: all_valid_layers() gracefully skips incompatible layers

let image = read()
    .no_deep_data()
    .largest_resolution_level()
    .rgb_channels(constructor, setter)
    .all_valid_layers()  // NEW
    .all_attributes()
    .from_file("mixed_layers.exr")?;

// Check what we got
if image.layer_data.is_empty() {
    println!("No layers matched RGB requirements");
} else {
    println!("Loaded {} RGB layers", image.layer_data.len());
}
```

**Benefits:**
- Graceful handling of mixed-format files
- Layer index mapping preserved
- Works with any channel reader

### Unified Deep/Flat Reader

Read any EXR without knowing the format:

```rust
use exr::prelude::*;

// New convenience function
let image = read_first_any_layer_from_file("mystery.exr")?;

// New enum for sample data
for channel in &image.layer_data.channel_data.list {
    match &channel.sample_data {
        DeepAndFlatSamples::Deep(deep) => {
            println!("{}: deep, {} samples", 
                channel.name, deep.total_samples());
        }
        DeepAndFlatSamples::Flat(flat) => {
            println!("{}: flat, {} samples", 
                channel.name, flat.len());
        }
    }
}

// Helper methods
channel.sample_data.is_deep();
channel.sample_data.is_flat();
```

## Compression Improvements

### Deep Data Compression

Extended compression module to handle deep data:

```rust
// New internal functions
compress_sample_table(compression, counts) -> Result<Vec<u8>>
decompress_sample_table(compression, data, width, height) -> Result<Vec<i32>>
compress_sample_data(compression, data, channels) -> Result<Vec<u8>>
decompress_sample_data(compression, data, ...) -> Result<DeepSamples>
```

### PIZ Improvements

- Fixed overflow bugs in Huffman coding
- Improved error handling
- Better bit manipulation
- +274 lines in `huffman.rs`
- +152 lines in `wavelet.rs`

### B44 Optimizations

- +10,928 lines of expanded lookup tables
- Faster encoding via precomputed values
- Better cache utilization

### General Compression

- Reduced allocations in hot paths
- Better memory locality
- Deep-aware byte reordering

## Block Layer Improvements

### Chunk Processing (`src/block/chunk.rs`)

+175 lines:
- Deep chunk type support
- Enhanced validation
- Improved error messages

### Lines Processing (`src/block/lines.rs`)

+100 lines:
- Subsampling considerations
- Deep line handling

### Sample Handling (`src/block/samples.rs`)

+189 lines:
- Deep sample types
- Type conversion improvements

### Reader (`src/block/reader.rs`)

+363 lines:
- Deep block reading
- Improved parallelization
- Better chunk iteration

### Writer (`src/block/writer.rs`)

+306 lines:
- Deep block writing
- Better chunk generation
- Offset table handling

## Metadata Improvements

### Attributes (`src/meta/attribute.rs`)

+956 lines:
- New attribute types for deep data
- Enhanced validation
- Improved parsing edge cases

### Headers (`src/meta/header.rs`)

+824 lines:
- Deep data header support
- `TileIndices` Ord/PartialOrd implementation
- Better level iteration
- Enhanced validation

### Meta Module (`src/meta/mod.rs`)

+547 lines:
- Deep metadata structures
- Version handling improvements
- Better error messages

## Image Module Improvements

### Crop Operations (`src/image/crop.rs`)

+698 lines:
- Deep image cropping support
- Better bounds checking
- Optimized memory handling

### Channel Operations

Various improvements:
- `write/channels.rs`: +323 lines
- `write/layers.rs`: +131 lines
- `write/samples.rs`: +134 lines
- `read/specific_channels.rs`: +350 lines

## I/O Improvements (`src/io.rs`)

+151 lines:
- Better seeking
- Overflow protection
- Improved error handling

## Math Utilities (`src/math.rs`)

+100 lines:
- Better vector operations
- Overflow-safe arithmetic

## Dead Code Cleanup

Systematic audit and cleanup:

| Item | Status |
|------|--------|
| `TileIndices::cmp` | Ord/PartialOrd implemented |
| `for_lines` unused | Deleted |
| Duplicate `enumerate_ordered_blocks` | Deleted |
| `ordered_block_indices` | Implemented + tests |
| `pixel_section_indices` | Implemented + 6 tests |
| `AnySamplesReader` | Roundtrip tested |
| `specific_resolution_level` | 10 tests |
| `all_valid_layers` | 5 tests |
| `validate_results` | `#[cfg(test)]` added |

**Result:** 9/11 items completed, 2 deferred (Rust limitations)

## Test Improvements

### New Test Files

1. **deep_read.rs** - 176 lines of deep reading tests
2. **deep_benchmark.rs** - 61 lines of performance benchmarks

### Enhanced Tests

- `roundtrip.rs`: +216 lines (deep roundtrip)
- `fuzz.rs`: +185 lines (deep fuzzing)
- `across_compression.rs`: +59 lines

### Test Data

Added official OpenEXR deep test images (~17MB):
- MiniCooper720p.exr
- PiranhnaAlienRun720p.exr
- Teaset720p.exr

## Examples Updates

All examples updated:
- Consistent error handling
- Deep data awareness
- Modern Rust idioms
- ~700 lines of improvements

## Documentation

### New Files

| File | Lines | Purpose |
|------|-------|---------|
| DEEP.md | 342 | Deep data format explanation |
| DEAD_CODE_ANALYSIS.md | 509 | Code quality audit |
| AGENTS.md | 556 | Architecture documentation |

### Updated Files

- README.md: +14 lines (deep data mention)
- GUIDE.md: +163 lines (deep data section)

## Performance Summary

| Area | Improvement |
|------|-------------|
| Deep parallel read | 1.5-2.5x speedup |
| B44 encoding | Faster via lookup tables |
| PIZ compression | Reduced allocations |
| Block iteration | Better cache usage |

## API Additions

### New Functions

```rust
// Unified reading
pub fn read_first_any_layer_from_file(path) -> Result<AnyImage>

// Deep reading  
pub fn read_first_deep_layer_from_file(path) -> Result<DeepImage>

// Level selection (builder method)
fn specific_resolution_level(selector) -> ReadSpecificLevel

// Valid layers (builder method)
fn all_valid_layers() -> ReadAllValidLayers
```

### New Types

```rust
// Deep data
pub struct DeepSamples { ... }
pub enum DeepChannelData { F16, F32, U32 }

// Unified
pub enum DeepAndFlatSamples { Deep(DeepSamples), Flat(FlatSamples) }

// Level info
pub struct LevelInfo { pub index: Vec2<usize>, pub resolution: Vec2<usize> }
```

### New Builder Methods

```rust
impl ReadBuilder {
    fn flat_and_deep_data(self) -> ReadAnySamples  // NEW
    fn any_deep_data(self) -> ReadDeepSamples      // NEW
}

impl ReadFlatSamples {
    fn specific_resolution_level<F>(self, selector: F) -> ReadSpecificLevel  // NEW
}

impl ReadChannels {
    fn all_valid_layers(self) -> ReadAllValidLayers  // NEW
}
```

## Backwards Compatibility

**All changes are backwards compatible.** Existing code continues to work without modification.

## See Also

- [Deep Data Support](./deep-data.md) - Deep-specific enhancements
- [Developer Guide](../../developer-guide/architecture.md) - Internal architecture
- [API Reference](../../api-reference/prelude.md) - Full API documentation
