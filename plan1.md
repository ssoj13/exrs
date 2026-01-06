# Bug Hunt Report - exrs OpenEXR Library

## Executive Summary

This is a comprehensive code quality audit of the `exrs` Rust library - a pure-Rust OpenEXR image format implementation. The codebase is well-architected overall, but has accumulated technical debt that should be addressed.

**Key Findings:**
- 150+ TODO/FIXME comments indicating unfinished work
- Several FIXME items indicate potential bugs
- Code duplication in compression modules
- Commented-out code blocks that need decisions
- Some architectural inconsistencies

---

## 1. Critical Issues (Potential Bugs)

### 1.1 Integer Overflow Bug in io.rs:228
```rust
// FIXME panicked at 'attempt to subtract with overflow'
let delta = target_position as i128 - self.position as i128;
```
**Status:** Known issue, needs proper fix with saturating arithmetic.
**Location:** `src/io.rs:228`

### 1.2 Subsampling Not Properly Implemented
Multiple locations indicate subsampling support is incomplete:
- `src/block/lines.rs:84` - "FIXME what about sub sampling??"
- `src/block/lines.rs:126` - "FIXME is it fewer samples per tile or just fewer tiles?"
- `src/block/mod.rs:176-227` - Multiple "TODO sampling??" comments
- `src/meta/attribute.rs:234-235` - "FIXME this needs to account for subsampling"
- `src/compression/mod.rs:273` - "FIXME this needs to account for subsampling"

**Impact:** Images with subsampled channels may produce incorrect results.

### 1.3 File Deletion on Error - Potential Data Loss
```rust
// src/io.rs:45
// FIXME deletes existing file if creation of new file fails?
```
**Impact:** May delete existing files unexpectedly.

### 1.4 DWA Compression Level Gets Lost
```rust
// src/meta/header.rs:1217
// FIXME dwa compression level gets lost if any other compression is used later in the process
```

### 1.5 Error Variant Possibly Unused
```rust
// src/error.rs:29
Aborted, // FIXME remove?? is not used really?
```

### 1.6 Huffman Decoder Issues
```rust
// src/compression/piz/huffman.rs:213-214
// FIXME why does this happen??
code_bit_count -= short_code.len(); // FIXME may throw "attempted to subtract with overflow"
```

### 1.7 Channel Groups Issues
```rust
// src/image/channel_groups.rs:227
// FIXME does not comply to `header.chunk_count` and that stuff??
// src/image/channel_groups.rs:231
// FIXME this is not the original order
```

---

## 2. Code Duplication

### 2.1 Compression Enum Variants Should Be Collapsed
From `src/compression/mod.rs`:
```rust
ZIP1,  // TODO collapse with ZIP16
ZIP16, // TODO ZIP { individual_lines: bool }

B44,   // TODO B44 { optimize_uniform_areas: bool }
B44A,  // TODO collapse with B44

DWAA(Option<f32>), // TODO collapse with DWAB
DWAB(Option<f32>),
```
**Recommendation:** Create unified variants with parameters.

### 2.2 Duplicate Merge Channel Functions
In `src/image/read/deep.rs`:
- `merge_channel_f16()`
- `merge_channel_f32()`
- `merge_channel_u32()`

These are nearly identical and could be generified.

### 2.3 Line Order Iteration Duplication
`src/meta/header.rs:393-420` - Same logic commented twice with "TODO without box?"

---

## 3. Dead/Commented-Out Code

### 3.1 Commented Implementations That Need Decisions:

| Location | Code Block | Decision Needed |
|----------|------------|-----------------|
| `src/meta/mod.rs:105-127` | `TileIndices::cmp()` | Remove or implement? |
| `src/block/mod.rs:247-264` | `lines_mut()`, `for_lines()` | Implement or delete? |
| `src/meta/header.rs:405-420` | Duplicate `enumerate_ordered_blocks` | Remove duplicate |
| `src/meta/header.rs:486-499` | `ordered_block_indices()` | Implement or delete? |
| `src/meta/attribute.rs:688-692` | `From<&str>` conflict | Resolve conflict |
| `src/meta/attribute.rs:768-785` | `pixel_section_indices()` | Implement for compression |
| `src/image/mod.rs:1351-1360` | Unnamed block | Decide on feature |
| `src/image/read/samples.rs:38-74` | `AnySamplesReader` | Implement or remove |
| `src/image/read/layers.rs:60` | `all_valid_layers()` | Implement or remove |

### 3.2 Test Code Possibly Shipped
```rust
// src/image/mod.rs:976
// #[cfg(test)] TODO do not ship this code
```

---

## 4. Architectural Issues

### 4.1 Naming Inconsistencies
Several TODO comments suggest renaming:
- `MetaData` -> `ImageInfo` (src/meta/mod.rs:21)
- `Header` -> `LayerDescription` (src/meta/header.rs:9)
- `headers` -> `layer_descriptions` (src/meta/mod.rs:34)

### 4.2 Type Parameters Awkwardness
```rust
// src/image/mod.rs:156
pub channels: ChannelsDescription, // TODO this is awkward. can this be not a type parameter please?
```

### 4.3 Box Allocations in Hot Paths
```rust
// src/meta/header.rs:393-394
// TODO without box?
let ordered: Box<dyn Send + Iterator<Item = (usize, TileIndices)>> = { ... }
```
Consider using enum dispatch instead.

### 4.4 Unnecessary Clones
Multiple locations noted:
- `src/meta/header.rs:1029` - "TODO no clone"
- `src/image/mod.rs:590` - "TODO no clone?"
- `src/image/write/channels.rs:141` - "TODO no clone?"
- `src/image/channel_groups.rs:222` - "TODO no clone?"

---

## 5. Performance Issues

### 5.1 Caching Recommendations
```rust
// src/meta/mod.rs:237, 243, 254, 270, 320
// TODO this should be cached? log2 may be very expensive
// TODO cache all these level values??
```

### 5.2 Unnecessary Allocations
```rust
// src/compression/b44/mod.rs:505
let uncompressed_le = uncompressed_le.as_slice(); // TODO no alloc

// src/compression/piz/mod.rs:176
let uncompressed_le = uncompressed_le.as_slice(); // TODO no alloc

// src/compression/pxr24.rs:43
let mut remaining_bytes_ne = bytes_ne.as_slice(); // TODO less allocation
```

### 5.3 Collection in Iterator Chain
```rust
// src/meta/header.rs:483
vec.into_iter() // TODO without collect
```

### 5.4 Memcpy Optimization Missing
```rust
// src/compression/b44/mod.rs:463, 556
// TODO simplify this and make it memcpy on little endian systems

// src/image/crop.rs:302
// TODO does this use memcpy?
```

---

## 6. Unsupported Features

### 6.1 Compression Methods Not Implemented
- DWAA/DWAB - Compression works, but:
  - No default value specified (`src/compression/mod.rs:123,130`)
  - Level parameter handling incomplete
- HTJ2K32, HTJ2K256 - Not supported (per README)

### 6.2 Deep Data Partial Support
- Writing deep data: partial (`src/image/write/deep.rs`)
- Deep tile support: needs testing
- Sample data validation: basic

### 6.3 Version 1 Files
```rust
// src/meta/mod.rs:63
// TODO write version 1 for simple images
```

---

## 7. API Improvements Needed

### 7.1 Static Lifetime Requirements Too Strict
```rust
// src/image/read/mod.rs:116, 149
// FIXME Set and Create should not need to be static
pub fn read_all_rgba_layers_from_file<..., Set: 'static, Create: 'static, Pixels: 'static>
```

### 7.2 Type Aliases Wanted
```rust
// src/image/read/mod.rs:127, 160
// TODO type alias? CreateRgbaPixels<Pixels=Pixels>
```

### 7.3 Parse Date Attribute
```rust
// src/meta/header.rs:157
pub capture_date: Option<Text>, // TODO parse!
```

---

## 8. Clippy Warnings

Current clippy run shows warnings for:
- `clippy::restriction` group enabled (should be individual lints)
- `pub use` in io.rs
- Single-character lifetime names (`'p`)
- Missing else branches
- Various style issues

---

## 9. Recommended Action Plan

### Phase 1: Critical Bug Fixes (High Priority)
1. [ ] Fix integer overflow in `io.rs:228`
2. [ ] Review and fix file deletion behavior in `io.rs:45`
3. [ ] Fix huffman overflow in `piz/huffman.rs:214`
4. [ ] Audit subsampling code paths

### Phase 2: Code Cleanup (Medium Priority)
1. [ ] Remove dead/commented code after decisions
2. [ ] Collapse compression enum variants
3. [ ] Fix clippy warnings
4. [ ] Remove unused `Aborted` error variant

### Phase 3: Deduplication (Medium Priority)
1. [ ] Generify merge_channel_* functions
2. [ ] Unify ZIP1/ZIP16, B44/B44A, DWAA/DWAB
3. [ ] Extract common iteration patterns

### Phase 4: Performance (Low Priority)
1. [ ] Add caching for level computations
2. [ ] Replace Box with enum dispatch
3. [ ] Eliminate unnecessary clones
4. [ ] Optimize memcpy paths for little-endian

### Phase 5: API Polish (Low Priority)
1. [ ] Rename types for clarity
2. [ ] Relax lifetime requirements
3. [ ] Add type aliases

---

## 10. Files Most Needing Attention

| File | Issues | Priority |
|------|--------|----------|
| `src/meta/header.rs` | 15+ TODOs, dead code | High |
| `src/compression/mod.rs` | Enum collapse, subsampling | High |
| `src/compression/piz/huffman.rs` | 2 FIXME bugs | High |
| `src/io.rs` | Overflow bug, deletion bug | High |
| `src/block/mod.rs` | Subsampling, dead code | Medium |
| `src/image/mod.rs` | Dead code, awkward types | Medium |
| `src/compression/b44/mod.rs` | 10+ TODOs, performance | Medium |
| `src/compression/piz/wavelet.rs` | Code cleanup | Low |

---

## Statistics

- **Total TODO comments:** ~130
- **Total FIXME comments:** ~20
- **Commented-out code blocks:** ~15
- **Rust files in src/:** 37
- **Lines of code:** ~15,000

---

*Report generated: 2026-01-05*
*Codebase version: 1.74.0*
