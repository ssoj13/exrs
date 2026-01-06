# Dead Code Analysis - exrs

## Overview

This document analyzes all commented-out code blocks and unused code in the exrs codebase to determine:
1. What each piece of code does
2. Whether it's an unfinished feature worth completing
3. Whether it should be deleted
4. Recommended action

---

## 1. TileIndices::cmp() - UNFINISHED FEATURE

**Location:** `src/meta/mod.rs:105-127`

```rust
/*impl TileIndices {
    pub fn cmp(&self, other: &Self) -> Ordering {
        match self.location.level_index.1.cmp(&other.location.level_index.1) {
            Ordering::Equal => {
                match self.location.level_index.0.cmp(&other.location.level_index.0) {
                    Ordering::Equal => {
                        match self.location.tile_index.1.cmp(&other.location.tile_index.1) {
                            Ordering::Equal => {
                                self.location.tile_index.0.cmp(&other.location.tile_index.0)
                            },
                            other => other,
                        }
                    },
                    other => other
                }
            },
            other => other
        }
    }
}*/
```

**What it does:**
Custom comparison function for `TileIndices` that establishes ordering:
1. First by Y level index
2. Then by X level index
3. Then by Y tile index
4. Finally by X tile index

This is "row-major" ordering for tiles across mip/rip map levels.

**Why it exists:**
Needed for sorting tiles in a specific order, likely for:
- Efficient file writing in line order
- Parallel processing optimization
- Implementing `Ord` trait for `TileIndices`

**Recommendation: COMPLETE THIS**
- This is useful for implementing `Ord` on `TileIndices`
- Would enable `tiles.sort()` instead of custom sorting
- Easy to implement: just derive `Ord` or uncomment and implement trait

**Action:**
```rust
impl Ord for TileIndices {
    fn cmp(&self, other: &Self) -> Ordering {
        // ... the commented code
    }
}
impl PartialOrd for TileIndices {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
```

---

## 2. UncompressedBlock::lines_mut() - UNFINISHED FEATURE

**Location:** `src/block/mod.rs:247-250`

```rust
/* TODO pub fn lines_mut<'s>(&'s mut self, header: &Header) -> impl 's + Iterator<Item=LineRefMut<'s>> {
    LineIndex::lines_in_block(self.index, &header.channels)
        .map(move |(bytes, line)| LineSlice { location: line, value: &mut self.data[bytes] })
}*/
```

**What it does:**
Returns a mutable iterator over lines in a block, allowing in-place modification of pixel data.

**Why it exists:**
The immutable version `lines()` exists at line 240. This would be the mutable counterpart for:
- In-place pixel transformations
- Efficient post-processing without allocation
- Filter pipelines

**Why it's commented:**
The borrow checker doesn't allow this pattern easily - the closure captures `self.data` mutably but also needs to access it multiple times. This is a common Rust lifetime challenge.

**Recommendation: DEFER or REDESIGN**
- The pattern doesn't work well with Rust's borrow checker
- Would need unsafe code or different approach (index-based API)
- Low priority - users can work with `data` directly

**Action:** Keep as documentation of attempted approach, or implement with:
```rust
pub fn line_mut(&mut self, index: usize, channels: &ChannelList) -> LineRefMut<'_> {
    // Single line access works fine
}
```

---

## 3. UncompressedBlock::for_lines() - REDUNDANT

**Location:** `src/block/mod.rs:252-264`

```rust
/*// TODO make iterator
/// Call a closure for each line of samples in this uncompressed block.
pub fn for_lines(
    &self, header: &Header,
    mut accept_line: impl FnMut(LineRef<'_>) -> UnitResult
) -> UnitResult {
    for (bytes, line) in LineIndex::lines_in_block(self.index, &header.channels) {
        let line_ref = LineSlice { location: line, value: &self.data[bytes] };
        accept_line(line_ref)?;
    }
    Ok(())
}*/
```

**What it does:**
Callback-based iteration over lines with early-exit on error.

**Why it's commented:**
The TODO says "make iterator" - this was replaced by `lines()` iterator.

**Recommendation: DELETE**
- `lines()` already exists and is more idiomatic
- Users can do: `for line in block.lines(&channels) { ... }`
- The error propagation can be done with `try_for_each`

**Action:** Delete this commented block.

---

## 4. Header::enumerate_ordered_blocks() DUPLICATE

**Location:** `src/meta/header.rs:405-420`

```rust
/*/// Iterate over all blocks, in the order specified by the headers line order attribute.
/// Also includes an index of the block if it were `LineOrder::Increasing`, starting at zero for this header.
pub fn enumerate_ordered_blocks(&self) -> impl Iterator<Item = (usize, TileIndices)> + Send {
    let increasing_y = self.blocks_increasing_y_order().enumerate();

    let ordered: Box<dyn Send + Iterator<Item = (usize, TileIndices)>> = {
        if self.line_order == LineOrder::Decreasing {
            Box::new(increasing_y.rev()) // TODO without box?
        }
        else {
            Box::new(increasing_y)
        }
    };

    ordered
}*/
```

**What it does:**
Exact duplicate of the function at lines 390-402 (which is active).

**Recommendation: DELETE**
- This is literally a copy of the active implementation
- Probably left over from editing

**Action:** Delete.

---

## 5. Header::ordered_block_indices() - UNFINISHED FEATURE

**Location:** `src/meta/header.rs:486-499`

```rust
/* TODO
/// The block indices of this header, ordered as they would appear in the file.
pub fn ordered_block_indices<'s>(&'s self, layer_index: usize) -> impl 's + Iterator<Item=BlockIndex> {
    self.enumerate_ordered_blocks().map(|(chunk_index, tile)|{
        let data_indices = self.get_absolute_block_pixel_coordinates(tile.location).expect("tile coordinate bug");

        BlockIndex {
            layer: layer_index,
            level: tile.location.level_index,
            pixel_position: data_indices.position.to_usize("data indices start").expect("data index bug"),
            pixel_size: data_indices.size,
        }
    })
}*/
```

**What it does:**
Returns `BlockIndex` objects instead of `TileIndices`, including the layer index. This is a higher-level API that combines tile info with layer info.

**Why it exists:**
Would simplify code in places that need `BlockIndex` instead of `TileIndices`. Currently this conversion is done inline in `enumerate_ordered_header_block_indices()` in `block/mod.rs`.

**Recommendation: COMPLETE THIS**
- Would reduce code duplication
- More ergonomic API
- The implementation is complete, just needs uncommenting

**Action:** Uncomment and use in `block/mod.rs:95-122`.

---

## 6. Text TryFrom<&str> - BLOCKED BY RUST LIMITATIONS

**Location:** `src/meta/attribute.rs:688-699`

```rust
/* TODO (currently conflicts with From<&str>)
impl<'s> TryFrom<&'s str> for Text {
    type Error = String;

    fn try_from(value: &'s str) -> std::result::Result<Self, Self::Error> {
        Text::new_or_none(value)
            .ok_or_else(|| format!(
                "exr::Text does not support all characters in the string `{}`",
                value
            ))
    }
}*/
```

**What it does:**
Fallible conversion from `&str` to `Text` that returns an error instead of panicking.

**Why it's commented:**
Rust's coherence rules don't allow both `From<&str>` and `TryFrom<&str>` because `From` implies `TryFrom`.

**Current situation:**
- `From<&str>` exists and panics on invalid input
- `Text::new_or_none()` exists as the fallible alternative

**Recommendation: KEEP AS-IS or CHANGE API**
Options:
1. Keep current: `From<&str>` (panics) + `new_or_none()` (returns Option)
2. Remove `From<&str>`, only have `TryFrom<&str>`
3. Keep as-is (current choice)

**Action:** Add doc comment explaining why this is commented.

---

## 7. ChannelList::pixel_section_indices() - UNFINISHED FEATURE

**Location:** `src/meta/attribute.rs:768-779`

```rust
// TODO use this in compression methods
/*pub fn pixel_section_indices(&self, bounds: IntegerBounds) -> impl '_ + Iterator<Item=(&Channel, usize, usize)> {
    (bounds.position.y() .. bounds.end().y()).flat_map(|y| {
        self.list
            .filter(|channel| mod_p(y, usize_to_i32(channel.sampling.1)) == 0)
            .flat_map(|channel|{
                (bounds.position.x() .. bounds.end().x())
                    .filter(|x| mod_p(*x, usize_to_i32(channel.sampling.0)) == 0)
                    .map(|x| (channel, x, y))
            })
    })
}*/
```

**What it does:**
Iterates over pixel positions respecting channel subsampling. For subsampled channels (like YUV 4:2:0), this skips positions where no sample exists.

**Why it exists:**
Needed for proper subsampling support in compression methods. Currently subsampling is marked with many `// TODO sampling??` comments.

**Recommendation: COMPLETE THIS - HIGH PRIORITY**
- This is needed to fix the subsampling bugs
- The implementation looks correct
- Would fix multiple FIXME comments in compression code

**Action:**
1. Uncomment and test
2. Use in compression methods
3. Fix all `// TODO sampling??` comments

---

## 8. AnySamplesReader - UNFINISHED FEATURE

**Location:** `src/image/read/samples.rs:38-41`

```rust
/*pub struct AnySamplesReader { TODO
    resolution: Vec2<usize>,
    samples: DeepAndFlatSamples
}*/
```

**What it does:**
Would be a reader that handles both deep and flat samples in a unified way.

**Why it exists:**
Currently there's `FlatSamplesReader` but no unified reader. This would allow reading any sample type without knowing in advance if it's deep or flat.

**Related:** `DeepAndFlatSamples` type doesn't exist yet.

**Recommendation: DEFER**
- Deep data support is still incomplete
- Would need `DeepAndFlatSamples` enum first
- Low priority until deep data is fully supported

**Action:** Keep as placeholder for future deep data unification.

---

## 9. specific_resolution_level() - ✅ COMPLETED

**Location:** `src/image/read/samples.rs` and `src/image/read/levels.rs`

**Status:** IMPLEMENTED (2026-01-06)

**Implementation:**
- Added `LevelInfo` struct to describe available resolution levels
- Added `ReadSpecificLevel<S, F>` struct for level selection
- Added `SpecificLevelReader` to filter and read selected level
- Added `specific_resolution_level()` method to `ReadFlatSamples`
- Added comprehensive tests (10 unit tests)
- Added rustdoc documentation with examples

**Usage:**
```rust
use exrs::prelude::*;

// Read mipmap level 1 (half resolution)
let image = read()
    .no_deep_data()
    .specific_resolution_level(|_| Vec2(1, 1))
    .all_channels()
    .first_valid_layer()
    .from_file("mipmapped.exr")?;

// Read level closest to 512x512
let image = read()
    .no_deep_data()
    .specific_resolution_level(|levels| {
        levels.iter()
            .min_by_key(|info| {
                let dx = (info.resolution.x() as i64 - 512).abs();
                let dy = (info.resolution.y() as i64 - 512).abs();
                dx + dy
            })
            .map(|info| info.index)
            .unwrap_or(Vec2(0, 0))
    })
    .all_channels()
    .first_valid_layer()
    .from_file("mipmapped.exr")?;
```

---

## 10. all_valid_layers() - ✅ COMPLETED

**Location:** `src/image/read/layers.rs`

**Status:** IMPLEMENTED (2026-01-06)

**Implementation:**
- Added `ReadAllValidLayers<C>` struct for reading valid layers
- Added `AllValidLayersReader<C>` with layer index mapping
- Added `all_valid_layers()` method to `ReadChannels` trait
- Uses `flat_map` pattern to skip invalid layers
- Added comprehensive tests (5 unit tests)
- Added rustdoc documentation with examples

**Usage:**
```rust
use exrs::prelude::*;

// Read all RGB layers, skip layers without RGB channels
let image = read()
    .no_deep_data()
    .largest_resolution_level()
    .rgb_channels(create_pixels, set_pixel)
    .all_valid_layers()  // Won't fail if some layers lack RGB
    .all_attributes()
    .from_file("mixed_layers.exr")?;

if image.layer_data.is_empty() {
    println!("No RGB layers found");
} else {
    println!("Found {} RGB layers", image.layer_data.len());
}
```

---

## 11. validate_results Module - TEST CODE IN PRODUCTION

**Location:** `src/image/mod.rs:974-1069+`

```rust
/// Compare the result of a round trip test with the original method.
/// Supports lossy compression methods.
// #[cfg(test)] TODO do not ship this code
pub mod validate_results {
    // ... ~300 lines of test utilities
}
```

**What it does:**
Test utilities for comparing images with tolerance for lossy compression. Includes:
- `ValidateResult` trait
- `ValidationOptions` struct
- Implementations for `Image`, `Layer`, channels, samples

**Why it's marked TODO:**
This is test code that shouldn't be in the public API.

**Current problem:**
- Module is `pub` - exposed to users
- Not gated by `#[cfg(test)]`
- Takes up space in compiled library

**Recommendation: FIX IMMEDIATELY**
Options:
1. Move to `tests/` directory
2. Gate with `#[cfg(test)]`
3. Make private and document as internal

**Action:**
```rust
#[cfg(test)]
pub(crate) mod validate_results { ... }
```

---

## Summary Table

| # | Location | Type | Priority | Action |
|---|----------|------|----------|--------|
| 1 | TileIndices::cmp | Unfinished | Medium | Complete - implement Ord |
| 2 | lines_mut | Blocked | Low | Defer - borrow checker issues |
| 3 | for_lines | Redundant | Low | Delete |
| 4 | enumerate_ordered_blocks dup | Duplicate | High | Delete |
| 5 | ordered_block_indices | Unfinished | Medium | Complete |
| 6 | TryFrom<&str> | Blocked | Low | Keep + document |
| 7 | pixel_section_indices | Unfinished | **HIGH** | Complete - fixes subsampling |
| 8 | AnySamplesReader | Placeholder | Low | Defer |
| 9 | specific_resolution_level | ✅ Completed | - | Implemented 2026-01-06 |
| 10 | all_valid_layers | ✅ Completed | - | Implemented 2026-01-06 |
| 11 | validate_results | Test leak | **HIGH** | Fix - add #[cfg(test)] |

---

## Recommended Action Plan

### Immediate (Bugfix)
1. **Delete duplicate** `enumerate_ordered_blocks` (item 4)
2. **Fix test leak** - add `#[cfg(test)]` to `validate_results` (item 11)

### Short-term (Subsampling fix)
3. **Complete `pixel_section_indices`** (item 7) - needed to fix subsampling bugs

### Medium-term (API improvements)
4. **Implement `Ord` for `TileIndices`** (item 1)
5. **Uncomment `ordered_block_indices`** (item 5)
6. **Delete redundant `for_lines`** (item 3)

### Long-term (Feature completion)
7. ~~Implement `specific_resolution_level` (item 9)~~ ✅ DONE
8. ~~Implement `all_valid_layers` (item 10)~~ ✅ DONE
9. Design solution for `lines_mut` (item 2)

### Defer indefinitely
10. `AnySamplesReader` - wait for deep data completion
11. `TryFrom<&str>` - keep current API

---

*Analysis completed: 2026-01-05*
*Updated: 2026-01-06 - Items 9 and 10 implemented*
