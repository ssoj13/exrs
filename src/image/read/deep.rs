//! Read deep EXR images from files or streams.
//!
//! # Overview
//!
//! Deep data (OpenEXR 2.0+) stores variable samples per pixel, requiring a separate
//! reading pipeline from flat images. This module provides:
//!
//! - **Convenience functions**: [`read_first_deep_layer_from_file()`], [`read_all_deep_layers_from_file()`]
//! - **Builder API**: [`read_deep()`] for full customization
//!
//! # Why a Separate Pipeline?
//!
//! Deep images differ from flat images in several ways:
//!
//! 1. **Variable geometry**: Sample count per pixel varies (0 to thousands)
//! 2. **Block format**: Uses `DeepScanLine`/`DeepTile` block types with packed offset tables
//! 3. **Two-pass reading**: Must read sample counts before allocating channel data
//! 4. **Merging complexity**: Scanline blocks must be merged into full-image representation
//!
//! The standard `read().no_deep_data()...` pipeline assumes fixed samples per pixel.
//!
//! # Reading Pipeline
//!
//! ```text
//! File
//!  │
//!  ├── MetaData (header with deep=true)
//!  │
//!  ├── DeepScanLine blocks (compressed)
//!  │    ├── y_coordinate
//!  │    ├── packed_offset_table (cumulative sample counts)
//!  │    └── sample_data (interleaved channels, little-endian)
//!  │
//!  └── Decompression → DeepSamples (per block)
//!                    ↓
//!              merge_deep_blocks()
//!                    ↓
//!              DeepSamples (full image)
//! ```
//!
//! # Block Merging
//!
//! Deep scanline files contain multiple blocks (typically 1-32 lines each).
//! [`merge_deep_blocks()`] combines them into a single [`DeepSamples`]:
//!
//! 1. Collect all blocks and sort by y-coordinate
//! 2. Build combined cumulative offset table (adding prefix sums)
//! 3. Allocate output channel arrays
//! 4. Copy sample data from each block at correct offsets
//!
//! This is the main complexity vs flat images which can directly use block data.
//!
//! # Usage Examples
//!
//! Simple file reading:
//! ```no_run
//! use exr::image::read::deep::read_first_deep_layer_from_file;
//!
//! let image = read_first_deep_layer_from_file("particles.exr")?;
//! let samples = &image.layer_data.channel_data.list[0].sample_data;
//!
//! println!("Total samples: {}", samples.total_samples());
//! println!("Max per pixel: {}", samples.max_samples_per_pixel());
//! # Ok::<(), exr::error::Error>(())
//! ```
//!
//! Builder API with options:
//! ```no_run
//! use exr::image::read::deep::read_deep;
//!
//! let image = read_deep()
//!     .all_channels()  // All channels as DeepSamples
//!     .first_valid_layer()  // First layer only
//!     .all_attributes()  // Keep all metadata
//!     .from_file("volumetric.exr")?;
//! # Ok::<(), exr::error::Error>(())
//! ```
//!
//! # Compression Support
//!
//! Deep data supports all standard compressions. Sample data is compressed separately
//! from the offset table. Most production files use ZIP compression.
//!
//! # See Also
//!
//! - [`crate::image::deep`] - Core [`DeepSamples`] type
//! - [`crate::image::write::deep`] - Writing deep images
//! - [`crate::block::deep`] - Block-level decompression
//! - [`crate::image::read`] - Flat image reading (non-deep)

use std::io::{BufReader, Read, Seek};
use std::path::Path;

use crate::block::chunk::CompressedBlock;
use crate::block::deep::{
    decompress_deep_scanline_block, decompress_deep_tile_block,
    SequentialDeepBlockDecompressor,
};
#[cfg(feature = "rayon")]
use crate::block::deep::ParallelDeepBlockDecompressor;
use crate::block::reader::Reader;
use crate::error::{Error, Result};
use crate::image::deep::DeepSamples;
use crate::image::{AnyChannel, AnyChannels, Blocks, Encoding, Image, Layer};
use crate::meta::header::Header;
use crate::meta::BlockDescription;
use smallvec::SmallVec;

// ============================================================================
// Types
// ============================================================================

/// A deep image with a single layer.
pub type DeepImage = Image<Layer<AnyChannels<DeepSamples>>>;

/// A deep image with multiple layers.
pub type DeepLayersImage = Image<crate::image::Layers<AnyChannels<DeepSamples>>>;

// ============================================================================
// Simple convenience functions (like read_first_flat_layer_from_file)
// ============================================================================

/// Read the first deep layer from a file.
/// Uses parallel decompression and relaxed error handling.
///
/// # Example
/// ```no_run
/// use exr::image::read::deep::read_first_deep_layer_from_file;
/// let image = read_first_deep_layer_from_file("deep.exr").unwrap();
/// ```
pub fn read_first_deep_layer_from_file(path: impl AsRef<Path>) -> Result<DeepImage> {
    read_deep()
        .all_channels()
        .first_valid_layer()
        .all_attributes()
        .from_file(path)
}

/// Read all deep layers from a file.
/// Uses parallel decompression and relaxed error handling.
pub fn read_all_deep_layers_from_file(path: impl AsRef<Path>) -> Result<DeepLayersImage> {
    read_deep()
        .all_channels()
        .all_layers()
        .all_attributes()
        .from_file(path)
}

// ============================================================================
// Builder pattern API (similar to read().no_deep_data()...)
// ============================================================================

/// Start building a deep image reader.
///
/// # Example
/// ```no_run
/// use exr::image::read::deep::read_deep;
/// let image = read_deep()
///     .all_channels()
///     .first_valid_layer()
///     .all_attributes()
///     .from_file("deep.exr").unwrap();
/// ```
pub fn read_deep() -> ReadDeepSamples {
    ReadDeepSamples
}

/// Specifies to read deep samples (variable samples per pixel).
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct ReadDeepSamples;

impl ReadDeepSamples {
    /// Read all channels from deep layers.
    pub fn all_channels(self) -> ReadDeepAllChannels {
        ReadDeepAllChannels
    }
}

/// Read all deep channels.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct ReadDeepAllChannels;

impl ReadDeepAllChannels {
    /// Read only the first valid deep layer.
    pub fn first_valid_layer(self) -> ReadDeepFirstLayer {
        ReadDeepFirstLayer
    }

    /// Read all deep layers.
    pub fn all_layers(self) -> ReadDeepAllLayers {
        ReadDeepAllLayers
    }
}

/// Read first deep layer.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct ReadDeepFirstLayer;

impl ReadDeepFirstLayer {
    /// Include all image attributes.
    pub fn all_attributes(self) -> ReadDeepImage<FirstLayer> {
        ReadDeepImage {
            pedantic: false,
            _parallel: cfg!(feature = "rayon"),
            _on_progress: None,
            _layer_selection: std::marker::PhantomData,
        }
    }
}

/// Read all deep layers.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct ReadDeepAllLayers;

impl ReadDeepAllLayers {
    /// Include all image attributes.
    pub fn all_attributes(self) -> ReadDeepImage<AllLayers> {
        ReadDeepImage {
            pedantic: false,
            _parallel: cfg!(feature = "rayon"),
            _on_progress: None,
            _layer_selection: std::marker::PhantomData,
        }
    }
}

/// Layer selection marker for first layer.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct FirstLayer;

/// Layer selection marker for all layers.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct AllLayers;

/// Final reader configuration for deep images.
#[derive(Debug, Clone)]
pub struct ReadDeepImage<LayerSelection> {
    pedantic: bool,
    _parallel: bool,
    _on_progress: Option<fn(f64)>,
    _layer_selection: std::marker::PhantomData<LayerSelection>,
}

impl<L> ReadDeepImage<L> {
    /// Use pedantic error handling.
    pub fn pedantic(mut self) -> Self {
        self.pedantic = true;
        self
    }

    /// Disable parallel decompression.
    pub fn non_parallel(mut self) -> Self {
        self._parallel = false;
        self
    }

    /// Set progress callback.
    pub fn on_progress(mut self, callback: fn(f64)) -> Self {
        self._on_progress = Some(callback);
        self
    }
}

impl ReadDeepImage<FirstLayer> {
    /// Read from file path.
    pub fn from_file(self, path: impl AsRef<Path>) -> Result<DeepImage> {
        self.from_unbuffered(std::fs::File::open(path)?)
    }

    /// Read from unbuffered reader.
    pub fn from_unbuffered(self, read: impl Read + Seek) -> Result<DeepImage> {
        self.from_buffered(BufReader::new(read))
    }

    /// Read from buffered reader.
    pub fn from_buffered(self, read: impl Read + Seek) -> Result<DeepImage> {
        let reader = Reader::read_from_buffered(read, self.pedantic)?;

        // Find first deep layer
        let (layer_index, _header) = reader
            .headers()
            .iter()
            .enumerate()
            .find(|(_, h)| h.deep)
            .ok_or_else(|| Error::invalid("no deep layer found"))?;

        let image_attrs = reader.headers()[layer_index].shared_attributes.clone();
        let layer = read_deep_layer_internal(reader, layer_index, self.pedantic, self._parallel)?;

        Ok(Image {
            attributes: image_attrs,
            layer_data: layer,
        })
    }
}

impl ReadDeepImage<AllLayers> {
    /// Read from file path.
    pub fn from_file(self, path: impl AsRef<Path>) -> Result<DeepLayersImage> {
        self.from_unbuffered(std::fs::File::open(path)?)
    }

    /// Read from unbuffered reader.
    pub fn from_unbuffered(self, read: impl Read + Seek) -> Result<DeepLayersImage> {
        self.from_buffered(BufReader::new(read))
    }

    /// Read from buffered reader.
    pub fn from_buffered(self, read: impl Read + Seek) -> Result<DeepLayersImage> {
        let reader = Reader::read_from_buffered(read, self.pedantic)?;

        // Collect deep layer indices
        let deep_indices: Vec<usize> = reader
            .headers()
            .iter()
            .enumerate()
            .filter(|(_, h)| h.deep)
            .map(|(i, _)| i)
            .collect();

        if deep_indices.is_empty() {
            return Err(Error::invalid("no deep layers found"));
        }

        let image_attrs = reader.headers()[deep_indices[0]].shared_attributes.clone();

        // For multiple layers, we need to read the file multiple times
        // This is not ideal but necessary since Reader consumes itself
        // TODO: Optimize by caching block data

        // For now, only support single deep layer in all_layers mode
        // to avoid re-reading the file multiple times
        if deep_indices.len() == 1 {
            let layer = read_deep_layer_internal(reader, deep_indices[0], self.pedantic, self._parallel)?;
            let mut layers = SmallVec::new();
            layers.push(layer);

            return Ok(Image {
                attributes: image_attrs,
                layer_data: layers,
            });
        }

        // Multiple layers - need to collect all blocks first
        let meta = reader.meta_data().clone();
        let chunks_reader = reader.all_chunks(self.pedantic)?;

        // Group blocks by layer
        let mut layer_blocks: Vec<Vec<(usize, DeepSamples)>> = vec![Vec::new(); meta.headers.len()];

        for chunk_result in chunks_reader {
            let chunk = chunk_result?;
            let layer_idx = chunk.layer_index;

            if !deep_indices.contains(&layer_idx) {
                continue;
            }

            let header = &meta.headers[layer_idx];
            let width = header.layer_size.width();
            let height = header.layer_size.height();

            match chunk.compressed_block {
                CompressedBlock::DeepScanLine(ref deep_block) => {
                    let y = deep_block.y_coordinate as usize;
                    let block_height = header
                        .compression
                        .scan_lines_per_block()
                        .min(height.saturating_sub(y));

                    let samples = decompress_deep_scanline_block(
                        deep_block,
                        header.compression,
                        &header.channels,
                        width,
                        block_height,
                        self.pedantic,
                    )?;

                    layer_blocks[layer_idx].push((y, samples));
                }
                CompressedBlock::DeepTile(ref deep_block) => {
                    let tile_size = match header.blocks {
                        BlockDescription::Tiles(desc) => desc.tile_size,
                        _ => return Err(Error::invalid("deep tile in scanline image")),
                    };

                    let samples = decompress_deep_tile_block(
                        deep_block,
                        header.compression,
                        &header.channels,
                        tile_size.width(),
                        tile_size.height(),
                        self.pedantic,
                    )?;

                    let y = deep_block.coordinates.tile_index.y() * tile_size.height();
                    layer_blocks[layer_idx].push((y, samples));
                }
                _ => {}
            }
        }

        // Build layers
        let mut layers = SmallVec::new();

        for layer_idx in deep_indices {
            let header = &meta.headers[layer_idx];
            let mut blocks = std::mem::take(&mut layer_blocks[layer_idx]);
            blocks.sort_by_key(|(y, _)| *y);

            let merged = merge_deep_blocks(
                blocks,
                header.layer_size.width(),
                header.layer_size.height(),
            )?;

            let layer = build_deep_layer(header, merged);
            layers.push(layer);
        }

        Ok(Image {
            attributes: image_attrs,
            layer_data: layers,
        })
    }
}

// ============================================================================
// Public helpers for unified reading
// ============================================================================

/// Read deep layer samples from a reader.
///
/// This is a low-level helper for unified deep/flat reading pipelines.
/// Returns just the channel data without the full [`Layer`] wrapper.
///
/// # Arguments
///
/// * `reader` - Reader positioned at start of file
/// * `layer_index` - Index of the deep layer to read
/// * `pedantic` - Use strict error handling
///
/// # Returns
///
/// [`AnyChannels<DeepSamples>`] containing all channels with their deep sample data.
///
/// # Errors
///
/// Returns error if:
/// - Layer index is out of bounds
/// - Layer is not a deep layer
/// - Decompression fails
///
/// # See Also
///
/// - [`crate::image::read::any_samples`] - Unified deep/flat reading
pub fn read_deep_layer_samples<R: Read + Seek>(
    reader: Reader<R>,
    layer_index: usize,
    pedantic: bool,
) -> Result<AnyChannels<DeepSamples>> {
    let parallel = cfg!(feature = "rayon");
    let layer = read_deep_layer_internal(reader, layer_index, pedantic, parallel)?;
    Ok(layer.channel_data)
}

// ============================================================================
// Internal implementation
// ============================================================================

/// Read a single deep layer from the reader.
fn read_deep_layer_internal<R: Read + Seek>(
    reader: Reader<R>,
    layer_index: usize,
    pedantic: bool,
    parallel: bool,
) -> Result<Layer<AnyChannels<DeepSamples>>> {
    let meta = reader.meta_data().clone();
    let header = &meta.headers[layer_index];
    let width = header.layer_size.width();
    let height = header.layer_size.height();

    let chunks_reader = reader.all_chunks(pedantic)?;

    // Collect blocks using parallel or sequential decompression
    let blocks = if parallel {
        #[cfg(feature = "rayon")]
        {
            decompress_blocks_parallel(chunks_reader, layer_index, pedantic)?
        }
        #[cfg(not(feature = "rayon"))]
        {
            decompress_blocks_sequential(chunks_reader, layer_index, pedantic)?
        }
    } else {
        decompress_blocks_sequential(chunks_reader, layer_index, pedantic)?
    };

    // Sort by y coordinate and merge
    let mut blocks = blocks;
    blocks.sort_by_key(|(y, _)| *y);
    let merged = merge_deep_blocks(blocks, width, height)?;

    Ok(build_deep_layer(&meta.headers[layer_index], merged))
}

/// Decompress blocks using parallel decompression (when rayon feature is enabled).
#[cfg(feature = "rayon")]
fn decompress_blocks_parallel<R: crate::block::reader::ChunksReader>(
    chunks: R,
    layer_index: usize,
    pedantic: bool,
) -> Result<Vec<(usize, DeepSamples)>> {
    let decompressor = match ParallelDeepBlockDecompressor::new(chunks, pedantic) {
        Ok(d) => d,
        Err(chunks) => {
            // Fall back to sequential if parallel not beneficial (e.g., uncompressed data)
            return decompress_blocks_sequential(chunks, layer_index, pedantic);
        }
    };

    let mut blocks = Vec::new();
    for block_result in decompressor {
        let block = block_result?;
        if block.layer_index != layer_index {
            continue;
        }
        blocks.push((block.y_coordinate as usize, block.samples));
    }
    Ok(blocks)
}

/// Decompress blocks sequentially (fallback or when rayon disabled).
fn decompress_blocks_sequential<R: crate::block::reader::ChunksReader>(
    chunks: R,
    layer_index: usize,
    pedantic: bool,
) -> Result<Vec<(usize, DeepSamples)>> {
    let decompressor = SequentialDeepBlockDecompressor::new(chunks, pedantic);

    let mut blocks = Vec::new();
    for block_result in decompressor {
        let block = block_result?;
        if block.layer_index != layer_index {
            continue;
        }
        blocks.push((block.y_coordinate as usize, block.samples));
    }
    Ok(blocks)
}

/// Build a Layer from header and DeepSamples.
fn build_deep_layer(header: &Header, samples: DeepSamples) -> Layer<AnyChannels<DeepSamples>> {
    // Build channel list - first channel gets the samples, rest get empty
    let channels: SmallVec<[AnyChannel<DeepSamples>; 4]> = header
        .channels
        .list
        .iter()
        .enumerate()
        .map(|(i, ch)| AnyChannel {
            name: ch.name.clone(),
            sample_data: if i == 0 {
                samples.clone()
            } else {
                DeepSamples::new(0, 0)
            },
            quantize_linearly: ch.quantize_linearly,
            sampling: ch.sampling,
        })
        .collect();

    Layer {
        channel_data: AnyChannels { list: channels },
        attributes: header.own_attributes.clone(),
        size: header.layer_size,
        encoding: Encoding {
            compression: header.compression,
            line_order: header.line_order,
            blocks: match header.blocks {
                BlockDescription::ScanLines => Blocks::ScanLines,
                BlockDescription::Tiles(desc) => Blocks::Tiles(desc.tile_size),
            },
        },
    }
}

/// Merge multiple deep scanline blocks into a single full-image [`DeepSamples`].
///
/// # Algorithm
///
/// Deep scanline files store data in blocks (typically 1-32 lines each).
/// This function combines them:
///
/// 1. **Build offset table**: Create `combined_offsets[total_pixels + 1]` with leading 0
/// 2. **Collect sample counts**: For each block, copy per-pixel counts to correct image positions
/// 3. **Prefix sum**: Convert individual counts to cumulative offsets
/// 4. **Allocate channels**: Create output arrays sized to `total_samples`
/// 5. **Copy data**: Use [`copy_block_samples()`] to place each block's data at correct positions
///
/// # Why Leading Zero?
///
/// The `combined_offsets` array has length `total_pixels + 1` with `combined_offsets[0] = 0`.
/// This simplifies the copy loop: for pixel N, destination starts at `combined_offsets[N]`.
/// The final `sample_offsets` returned to [`DeepSamples`] is `combined_offsets[1..]`.
///
/// # Performance
///
/// - O(total_pixels) for offset computation
/// - O(total_samples * num_channels) for data copying
/// - Single allocation per channel (no resizing)
///
/// # Arguments
///
/// * `blocks` - Vec of (y_offset, DeepSamples) pairs, one per block
/// * `total_width` - Full image width
/// * `total_height` - Full image height
fn merge_deep_blocks(
    blocks: Vec<(usize, DeepSamples)>,
    total_width: usize,
    total_height: usize,
) -> Result<DeepSamples> {
    if blocks.is_empty() {
        return Ok(DeepSamples::new(total_width, total_height));
    }

    if blocks.len() == 1 {
        let (_, samples) = blocks.into_iter().next().unwrap();
        return Ok(samples);
    }

    // Multiple blocks - merge sample counts and channel data
    let total_pixels = total_width * total_height;
    let mut combined_offsets = vec![0u32; total_pixels + 1];

    // Calculate sample counts from all blocks
    for (y, block) in &blocks {
        for row in 0..block.height {
            let image_y = y + row;
            if image_y >= total_height {
                break;
            }

            for col in 0..block.width.min(total_width) {
                let pixel_idx = image_y * total_width + col;
                combined_offsets[pixel_idx + 1] = block.sample_count(col, row) as u32;
            }
        }
    }

    // Convert to cumulative
    for i in 0..total_pixels {
        combined_offsets[i + 1] += combined_offsets[i];
    }

    let total_samples = combined_offsets[total_pixels] as usize;
    let num_channels = blocks.first().map(|(_, b)| b.channels.len()).unwrap_or(0);

    // Merge channel data
    use crate::image::deep::DeepChannelData;
    use crate::meta::attribute::SampleType;

    let mut combined_channels = Vec::with_capacity(num_channels);

    for ch_idx in 0..num_channels {
        let sample_type = blocks
            .first()
            .and_then(|(_, b)| b.channels.get(ch_idx))
            .map(|ch| ch.sample_type());

        match sample_type {
            Some(SampleType::F16) => {
                let mut data = vec![half::f16::ZERO; total_samples];
                merge_channel_f16(
                    &blocks,
                    ch_idx,
                    total_width,
                    total_height,
                    &combined_offsets,
                    &mut data,
                );
                combined_channels.push(DeepChannelData::F16(data));
            }
            Some(SampleType::F32) => {
                let mut data = vec![0.0f32; total_samples];
                merge_channel_f32(
                    &blocks,
                    ch_idx,
                    total_width,
                    total_height,
                    &combined_offsets,
                    &mut data,
                );
                combined_channels.push(DeepChannelData::F32(data));
            }
            Some(SampleType::U32) => {
                let mut data = vec![0u32; total_samples];
                merge_channel_u32(
                    &blocks,
                    ch_idx,
                    total_width,
                    total_height,
                    &combined_offsets,
                    &mut data,
                );
                combined_channels.push(DeepChannelData::U32(data));
            }
            None => {}
        }
    }

    // combined_offsets has length total_pixels + 1 with leading 0
    // sample_offsets expects length total_pixels without leading 0
    Ok(DeepSamples {
        sample_offsets: combined_offsets[1..].to_vec(),
        channels: combined_channels,
        width: total_width,
        height: total_height,
    })
}

/// Merge F16 channel data.
fn merge_channel_f16(
    blocks: &[(usize, DeepSamples)],
    ch_idx: usize,
    total_width: usize,
    total_height: usize,
    combined_offsets: &[u32],
    output: &mut [half::f16],
) {
    use crate::image::deep::DeepChannelData;

    for (y, block) in blocks {
        let src = match block.channels.get(ch_idx) {
            Some(DeepChannelData::F16(v)) => v,
            _ => continue,
        };

        copy_block_samples(
            block,
            *y,
            src,
            total_width,
            total_height,
            combined_offsets,
            output,
        );
    }
}

/// Merge F32 channel data.
fn merge_channel_f32(
    blocks: &[(usize, DeepSamples)],
    ch_idx: usize,
    total_width: usize,
    total_height: usize,
    combined_offsets: &[u32],
    output: &mut [f32],
) {
    use crate::image::deep::DeepChannelData;

    for (y, block) in blocks {
        let src = match block.channels.get(ch_idx) {
            Some(DeepChannelData::F32(v)) => v,
            _ => continue,
        };

        copy_block_samples(
            block,
            *y,
            src,
            total_width,
            total_height,
            combined_offsets,
            output,
        );
    }
}

/// Merge U32 channel data.
fn merge_channel_u32(
    blocks: &[(usize, DeepSamples)],
    ch_idx: usize,
    total_width: usize,
    total_height: usize,
    combined_offsets: &[u32],
    output: &mut [u32],
) {
    use crate::image::deep::DeepChannelData;

    for (y, block) in blocks {
        let src = match block.channels.get(ch_idx) {
            Some(DeepChannelData::U32(v)) => v,
            _ => continue,
        };

        copy_block_samples(
            block,
            *y,
            src,
            total_width,
            total_height,
            combined_offsets,
            output,
        );
    }
}

/// Copy samples from a block to the combined output.
fn copy_block_samples<T: Copy>(
    block: &DeepSamples,
    y_offset: usize,
    src: &[T],
    total_width: usize,
    total_height: usize,
    combined_offsets: &[u32],
    output: &mut [T],
) {
    for row in 0..block.height {
        let image_y = y_offset + row;
        if image_y >= total_height {
            break;
        }

        for col in 0..block.width.min(total_width) {
            let pixel_idx = image_y * total_width + col;
            let block_pixel_idx = row * block.width + col;
            let count = block.sample_count(col, row);

            if count == 0 {
                continue;
            }

            // Get source range from block
            let (src_start, _) = block.sample_range(block_pixel_idx);
            // combined_offsets has leading 0, so combined_offsets[N] = cumulative count before pixel N
            let dst_start = combined_offsets[pixel_idx] as usize;

            for s in 0..count {
                if src_start + s < src.len() && dst_start + s < output.len() {
                    output[dst_start + s] = src[src_start + s];
                }
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_read_deep_first_layer() {
        let path = "tests/images/valid/openexr/v2/LowResLeftView/Balls.exr";
        if !std::path::Path::new(path).exists() {
            eprintln!("Skipping: {} not found", path);
            return;
        }

        let image = match read_first_deep_layer_from_file(path) {
            Ok(img) => img,
            Err(e) => {
                eprintln!("Error reading {}: {:?}", path, e);
                panic!("Failed to read: {}", e);
            }
        };

        assert!(image.layer_data.size.width() > 0);
        assert!(image.layer_data.size.height() > 0);

        let samples = &image.layer_data.channel_data.list[0].sample_data;
        assert!(samples.total_samples() > 0);

        println!("Size: {:?}", image.layer_data.size);
        println!("Channels: {}", image.layer_data.channel_data.list.len());
        println!("Total samples: {}", samples.total_samples());
    }

    #[test]
    fn test_read_deep_builder_api() {
        let path = "tests/images/valid/openexr/v2/LowResLeftView/Ground.exr";
        if !std::path::Path::new(path).exists() {
            eprintln!("Skipping: {} not found", path);
            return;
        }

        let image = read_deep()
            .all_channels()
            .first_valid_layer()
            .all_attributes()
            .from_file(path)
            .unwrap();

        let samples = &image.layer_data.channel_data.list[0].sample_data;
        println!("Ground.exr: {} samples", samples.total_samples());
    }

    #[test]
    fn test_read_via_main_api() {
        let path = "tests/images/valid/openexr/v2/LowResLeftView/Leaves.exr";
        if !std::path::Path::new(path).exists() {
            eprintln!("Skipping: {} not found", path);
            return;
        }

        // Test using main read() API
        use crate::image::read::read;
        let image = read()
            .deep_data()
            .all_channels()
            .first_valid_layer()
            .all_attributes()
            .from_file(path)
            .unwrap();

        let samples = &image.layer_data.channel_data.list[0].sample_data;
        println!("Leaves.exr via read(): {} samples", samples.total_samples());
    }
}
