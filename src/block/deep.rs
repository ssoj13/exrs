//! Deep data block processing - decompression and compression of deep scanline/tile blocks.
//!
//! # Overview
//!
//! This module handles the low-level conversion between compressed deep blocks
//! (as stored in EXR files) and in-memory [`DeepSamples`] representation.
//!
//! # Block Format
//!
//! OpenEXR deep blocks contain two distinct data sections:
//!
//! ```text
//! CompressedDeepScanLineBlock / CompressedDeepTileBlock
//! ├── y_coordinate / tile_coordinates
//! ├── compressed_pixel_offset_table (sample counts as cumulative i32)
//! ├── compressed_sample_data_le (interleaved channel values)
//! └── decompressed_sample_data_size (for validation)
//! ```
//!
//! ## Sample Offset Table
//!
//! The offset table stores **cumulative sample counts per scanline**.
//! Unlike [`DeepSamples::sample_offsets`] which is per-pixel, OpenEXR files
//! store per-line cumulative counts that restart at 0 for each line.
//!
//! For a 3-pixel-wide block with 2 lines:
//! ```text
//! Per-pixel counts:   [2, 1, 3]  [0, 2, 1]
//! File offset table:  [2, 3, 6,   0, 2, 3]  <- restarts each line!
//! ```
//!
//! ## Sample Data Layout
//!
//! Samples are interleaved by pixel, then by channel (SoA within each pixel):
//! ```text
//! Pixel 0: [ch0_s0, ch0_s1], [ch1_s0, ch1_s1], [ch2_s0, ch2_s1]
//! Pixel 1: [ch0_s0], [ch1_s0], [ch2_s0]
//! ... etc
//! ```
//!
//! All values are little-endian.
//!
//! # Decompression Pipeline
//!
//! 1. Decompress offset table via [`crate::compression::deep::decompress_sample_table()`]
//! 2. Convert per-line offsets to per-pixel cumulative
//! 3. Decompress sample data via [`crate::compression::deep::decompress_sample_data()`]
//! 4. Unpack interleaved bytes into typed channel arrays
//!
//! # Compression Pipeline
//!
//! 1. Pack typed channel arrays into interleaved bytes
//! 2. Compress sample data
//! 3. Build per-line offset table from cumulative counts
//! 4. Compress offset table
//!
//! # See Also
//!
//! - [`crate::compression::deep`] - Compression/decompression algorithms
//! - [`crate::image::deep`] - High-level [`DeepSamples`] type
//! - [`crate::block::chunk`] - Block types (`CompressedDeepScanLineBlock`)

use crate::block::chunk::{CompressedDeepScanLineBlock, CompressedDeepTileBlock};
use crate::compression::{deep as deep_compress, Compression};
use crate::error::{Error, Result};
use crate::image::deep::{DeepChannelData, DeepSamples};
use crate::meta::attribute::{ChannelList, SampleType};
use half::f16;

/// Decompress a deep scanline block into [`DeepSamples`].
///
/// This is the main entry point for reading deep scanline data from files.
///
/// # Process
///
/// 1. Decompress the sample count offset table (ZIP/RLE/etc)
/// 2. Validate that counts are monotonically non-decreasing
/// 3. Decompress the raw sample data bytes
/// 4. Unpack bytes into typed channel arrays (f16/f32/u32)
///
/// # Arguments
///
/// * `block` - Compressed block from file
/// * `compression` - Compression method (from header)
/// * `channels` - Channel list defining types and order
/// * `data_window_width` - Block width (usually image width for scanlines)
/// * `lines_per_block` - Number of scanlines in this block
/// * `pedantic` - If true, fail on minor format violations
///
/// # Returns
///
/// [`DeepSamples`] with decompressed data, or error if data is malformed.
pub fn decompress_deep_scanline_block(
    block: &CompressedDeepScanLineBlock,
    compression: Compression,
    channels: &ChannelList,
    data_window_width: usize,
    lines_per_block: usize,
    pedantic: bool,
) -> Result<DeepSamples> {
    let width = data_window_width;
    let height = lines_per_block;

    // Decompress sample count table
    let table_bytes: Vec<u8> = block
        .compressed_pixel_offset_table
        .iter()
        .map(|&b| b as u8)
        .collect();

    let cumulative_counts =
        deep_compress::decompress_sample_table(compression, &table_bytes, width, height, pedantic)?;

    // Validate counts
    deep_compress::validate_sample_table(&cumulative_counts)?;

    // Create DeepSamples structure
    let mut samples = DeepSamples::new(width, height);

    // Convert i32 cumulative to u32
    let cumulative_u32: Vec<u32> = cumulative_counts.iter().map(|&c| c as u32).collect();

    samples.set_cumulative_counts(cumulative_u32)?;

    // Decompress sample data
    let decompressed_data = deep_compress::decompress_sample_data(
        compression,
        &block.compressed_sample_data_le,
        block.decompressed_sample_data_size,
        pedantic,
    )?;

    // Unpack channel data
    unpack_deep_channels(&decompressed_data, &mut samples, channels)?;

    samples.validate()?;
    Ok(samples)
}

/// Decompress a deep tile block into DeepSamples.
pub fn decompress_deep_tile_block(
    block: &CompressedDeepTileBlock,
    compression: Compression,
    channels: &ChannelList,
    tile_width: usize,
    tile_height: usize,
    pedantic: bool,
) -> Result<DeepSamples> {
    // Decompress sample count table
    let table_bytes: Vec<u8> = block
        .compressed_pixel_offset_table
        .iter()
        .map(|&b| b as u8)
        .collect();

    let cumulative_counts = deep_compress::decompress_sample_table(
        compression,
        &table_bytes,
        tile_width,
        tile_height,
        pedantic,
    )?;

    // Validate counts
    deep_compress::validate_sample_table(&cumulative_counts)?;

    // Create DeepSamples structure
    let mut samples = DeepSamples::new(tile_width, tile_height);

    let cumulative_u32: Vec<u32> = cumulative_counts.iter().map(|&c| c as u32).collect();

    samples.set_cumulative_counts(cumulative_u32)?;

    // Decompress sample data
    let decompressed_data = deep_compress::decompress_sample_data(
        compression,
        &block.compressed_sample_data_le,
        block.decompressed_sample_data_size,
        pedantic,
    )?;

    // Unpack channel data
    unpack_deep_channels(&decompressed_data, &mut samples, channels)?;

    samples.validate()?;
    Ok(samples)
}

/// Unpack decompressed bytes into DeepSamples channels.
/// Data layout: for each pixel, for each sample, for each channel - channel value in LE format.
fn unpack_deep_channels(
    data: &[u8],
    samples: &mut DeepSamples,
    channels: &ChannelList,
) -> Result<()> {
    let total_samples = samples.total_samples();

    if total_samples == 0 {
        // No samples, just allocate empty channels
        samples.allocate_channels(channels);
        return Ok(());
    }

    // Allocate channel storage
    samples.allocate_channels(channels);

    // Calculate bytes per sample (sum of all channel bytes)
    let bytes_per_sample: usize = channels
        .list
        .iter()
        .map(|ch| ch.sample_type.bytes_per_sample())
        .sum();

    let expected_size = total_samples * bytes_per_sample;
    if data.len() != expected_size {
        return Err(Error::invalid(format!(
            "deep sample data size mismatch: got {}, expected {} ({} samples * {} bytes)",
            data.len(),
            expected_size,
            total_samples,
            bytes_per_sample
        )));
    }

    // Deep data is stored pixel-interleaved:
    // For each pixel, for each sample in that pixel, for each channel: value
    //
    // We need to distribute samples to channels in SoA format.
    let mut data_offset = 0;
    let pixel_count = samples.pixel_count();

    for pixel_idx in 0..pixel_count {
        let (start, end) = samples.sample_range(pixel_idx);
        let sample_count = end - start;

        for sample_idx in 0..sample_count {
            let dest_idx = start + sample_idx;

            for (ch_idx, channel_desc) in channels.list.iter().enumerate() {
                let channel_data = &mut samples.channels[ch_idx];

                match channel_desc.sample_type {
                    SampleType::F16 => {
                        let bytes = [data[data_offset], data[data_offset + 1]];
                        let value = f16::from_le_bytes(bytes);
                        if let DeepChannelData::F16(ref mut v) = channel_data {
                            v[dest_idx] = value;
                        }
                        data_offset += 2;
                    }
                    SampleType::F32 => {
                        let bytes = [
                            data[data_offset],
                            data[data_offset + 1],
                            data[data_offset + 2],
                            data[data_offset + 3],
                        ];
                        let value = f32::from_le_bytes(bytes);
                        if let DeepChannelData::F32(ref mut v) = channel_data {
                            v[dest_idx] = value;
                        }
                        data_offset += 4;
                    }
                    SampleType::U32 => {
                        let bytes = [
                            data[data_offset],
                            data[data_offset + 1],
                            data[data_offset + 2],
                            data[data_offset + 3],
                        ];
                        let value = u32::from_le_bytes(bytes);
                        if let DeepChannelData::U32(ref mut v) = channel_data {
                            v[dest_idx] = value;
                        }
                        data_offset += 4;
                    }
                }
            }
        }
    }

    debug_assert_eq!(data_offset, data.len(), "not all deep data was consumed");
    Ok(())
}

/// Pack DeepSamples channels into bytes for compression.
/// Returns the data in pixel-interleaved LE format.
pub fn pack_deep_channels(samples: &DeepSamples, channels: &ChannelList) -> Vec<u8> {
    let total_samples = samples.total_samples();

    if total_samples == 0 {
        return Vec::new();
    }

    let bytes_per_sample: usize = channels
        .list
        .iter()
        .map(|ch| ch.sample_type.bytes_per_sample())
        .sum();

    let mut data = Vec::with_capacity(total_samples * bytes_per_sample);
    let pixel_count = samples.pixel_count();

    for pixel_idx in 0..pixel_count {
        let (start, end) = samples.sample_range(pixel_idx);
        let sample_count = end - start;

        for sample_idx in 0..sample_count {
            let src_idx = start + sample_idx;

            for (ch_idx, channel_desc) in channels.list.iter().enumerate() {
                let channel_data = &samples.channels[ch_idx];

                match channel_desc.sample_type {
                    SampleType::F16 => {
                        if let DeepChannelData::F16(ref v) = channel_data {
                            data.extend_from_slice(&v[src_idx].to_le_bytes());
                        }
                    }
                    SampleType::F32 => {
                        if let DeepChannelData::F32(ref v) = channel_data {
                            data.extend_from_slice(&v[src_idx].to_le_bytes());
                        }
                    }
                    SampleType::U32 => {
                        if let DeepChannelData::U32(ref v) = channel_data {
                            data.extend_from_slice(&v[src_idx].to_le_bytes());
                        }
                    }
                }
            }
        }
    }

    data
}

/// Compress DeepSamples into a CompressedDeepScanLineBlock.
pub fn compress_deep_scanline_block(
    samples: &DeepSamples,
    compression: Compression,
    channels: &ChannelList,
    y_coordinate: i32,
) -> Result<CompressedDeepScanLineBlock> {
    // Get cumulative counts as i32
    let cumulative_i32: Vec<i32> = samples.sample_offsets.iter().map(|&c| c as i32).collect();

    // Compress sample count table
    let compressed_table = deep_compress::compress_sample_table(compression, &cumulative_i32)?;

    // Pack and compress sample data
    let packed_data = pack_deep_channels(samples, channels);
    let decompressed_size = packed_data.len();

    let compressed_data = deep_compress::compress_sample_data(compression, &packed_data)?;

    Ok(CompressedDeepScanLineBlock {
        y_coordinate,
        decompressed_sample_data_size: decompressed_size,
        compressed_pixel_offset_table: compressed_table.iter().map(|&b| b as i8).collect(),
        compressed_sample_data_le: compressed_data,
    })
}

/// Compress DeepSamples into a CompressedDeepTileBlock.
pub fn compress_deep_tile_block(
    samples: &DeepSamples,
    compression: Compression,
    channels: &ChannelList,
    coordinates: crate::block::chunk::TileCoordinates,
) -> Result<CompressedDeepTileBlock> {
    // Get cumulative counts as i32
    let cumulative_i32: Vec<i32> = samples.sample_offsets.iter().map(|&c| c as i32).collect();

    // Compress sample count table
    let compressed_table = deep_compress::compress_sample_table(compression, &cumulative_i32)?;

    // Pack and compress sample data
    let packed_data = pack_deep_channels(samples, channels);
    let decompressed_size = packed_data.len();

    let compressed_data = deep_compress::compress_sample_data(compression, &packed_data)?;

    Ok(CompressedDeepTileBlock {
        coordinates,
        decompressed_sample_data_size: decompressed_size,
        compressed_pixel_offset_table: compressed_table.iter().map(|&b| b as i8).collect(),
        compressed_sample_data_le: compressed_data,
    })
}

// ============================================================================
// Parallel Deep Block Decompression
// ============================================================================

/// A decompressed deep block ready for assembly into `DeepImage`.
///
/// This is the deep data equivalent of [`UncompressedBlock`](super::UncompressedBlock),
/// produced by parallel decompression.
#[derive(Debug, Clone)]
pub struct DeepUncompressedBlock {
    /// Layer index this block belongs to.
    pub layer_index: usize,

    /// Y coordinate for scanline blocks, or tile coordinates for tiled.
    pub y_coordinate: i32,

    /// The decompressed deep samples.
    pub samples: DeepSamples,
}

/// # Architectural Design Notes: Parallel Deep Data Processing
///
/// When implementing parallel deep block decompression, several approaches were considered.
/// This documents the trade-offs to aid future maintenance.
///
/// ## Comparison of Approaches
///
/// | Approach | Breaking Changes | Complexity | Code Reuse |
/// |----------|------------------|------------|------------|
/// | A. Extend `UncompressedBlock` | High - affects all read code | Medium | Maximum |
/// | **B. Separate `ParallelDeepBlockDecompressor`** | **None** | **Medium** | **Moderate** |
/// | C. Generic trait `ParallelDecompressor<T>` | Low | High | Maximum |
/// | D. Simple `par_iter` in high-level | None | Low | Minimal |
///
/// ## Option A: Extend UncompressedBlock (Rejected)
///
/// Add `DeepSamples` variant to existing `UncompressedBlock`:
/// ```ignore
/// pub enum UncompressedBlock {
///     ScanLine { ... },
///     Tile { ... },
///     DeepScanLine { y: i32, samples: DeepSamples },  // NEW
///     DeepTile { coords: TileCoordinates, samples: DeepSamples },  // NEW
/// }
/// ```
/// **Pros:** Maximum code reuse, single parallel decompressor.
/// **Cons:** Breaking change to public API, adds deep handling to all consumers.
///
/// ## Option B: Separate ParallelDeepBlockDecompressor (Chosen)
///
/// Create dedicated `ParallelDeepBlockDecompressor` following existing pattern:
/// ```ignore
/// pub struct ParallelDeepBlockDecompressor<R: ChunksReader> { ... }
/// impl Iterator for ParallelDeepBlockDecompressor<R> {
///     type Item = Result<DeepUncompressedBlock>;
/// }
/// ```
/// **Pros:** No breaking changes, clear separation, follows established pattern.
/// **Cons:** Some code duplication with `ParallelBlockDecompressor`.
///
/// ## Option C: Generic Trait (Rejected)
///
/// Abstract over block type with trait:
/// ```ignore
/// trait DecompressibleBlock: Send + 'static {
///     fn decompress(chunk: Chunk, meta: &MetaData) -> Result<Self>;
/// }
/// struct ParallelDecompressor<R, B: DecompressibleBlock> { ... }
/// ```
/// **Pros:** Maximum flexibility and reuse.
/// **Cons:** Over-engineering for two use cases, complex trait bounds.
///
/// ## Option D: Simple par_iter (Rejected)
///
/// Just use rayon's `par_iter` at high level:
/// ```ignore
/// let blocks: Vec<_> = chunks.par_iter()
///     .map(|c| decompress_deep_block(c))
///     .collect()?;
/// ```
/// **Pros:** Simplest implementation.
/// **Cons:** No streaming, high memory for large files, doesn't follow existing pattern.
///
/// ## Implementation Decision
///
/// Option B was chosen because:
/// 1. **API stability** - No changes to existing `UncompressedBlock` consumers
/// 2. **Pattern consistency** - Mirrors `ParallelBlockDecompressor` exactly
/// 3. **Clear ownership** - Deep-specific code stays in `block::deep`
/// 4. **Streaming** - Supports memory-efficient block-by-block processing
#[cfg(feature = "rayon")]
#[derive(Debug)]
pub struct ParallelDeepBlockDecompressor<R: super::reader::ChunksReader> {
    remaining_chunks: R,
    sender: std::sync::mpsc::Sender<Result<DeepUncompressedBlock>>,
    receiver: std::sync::mpsc::Receiver<Result<DeepUncompressedBlock>>,
    currently_decompressing_count: usize,
    max_threads: usize,
    shared_meta_data_ref: std::sync::Arc<crate::meta::MetaData>,
    pedantic: bool,
    pool: rayon_core::ThreadPool,
}

#[cfg(feature = "rayon")]
impl<R: super::reader::ChunksReader> ParallelDeepBlockDecompressor<R> {
    /// Create a new parallel deep block decompressor.
    ///
    /// Returns `Err(chunks)` if parallel decompression is not beneficial
    /// (e.g., all data is uncompressed).
    pub fn new(chunks: R, pedantic: bool) -> std::result::Result<Self, R> {
        Self::new_with_thread_pool(chunks, pedantic, || {
            rayon_core::ThreadPoolBuilder::new()
                .thread_name(|index| format!("Deep Block Decompressor #{}", index))
                .build()
        })
    }

    /// Create with a custom thread pool builder.
    pub fn new_with_thread_pool<CreatePool>(
        chunks: R,
        pedantic: bool,
        try_create_thread_pool: CreatePool,
    ) -> std::result::Result<Self, R>
    where
        CreatePool:
            FnOnce()
                -> std::result::Result<rayon_core::ThreadPool, rayon_core::ThreadPoolBuildError>,
    {
        use crate::compression::Compression;

        // Check if all layers are uncompressed - no benefit from parallelism
        let is_entirely_uncompressed = chunks
            .meta_data()
            .headers
            .iter()
            .all(|head| head.compression == Compression::Uncompressed);

        if is_entirely_uncompressed {
            return Err(chunks);
        }

        let pool = match try_create_thread_pool() {
            Ok(pool) => pool,
            Err(_) => return Err(chunks),
        };

        let max_threads = pool.current_num_threads().max(1).min(chunks.len()) + 2;
        let (send, recv) = std::sync::mpsc::channel();

        Ok(Self {
            shared_meta_data_ref: std::sync::Arc::new(chunks.meta_data().clone()),
            currently_decompressing_count: 0,
            remaining_chunks: chunks,
            sender: send,
            receiver: recv,
            pedantic,
            max_threads,
            pool,
        })
    }

    /// Decompress the next block, spawning parallel jobs as needed.
    pub fn decompress_next_block(&mut self) -> Option<Result<DeepUncompressedBlock>> {
        // Fill thread pool with jobs
        while self.currently_decompressing_count < self.max_threads {
            let chunk = self.remaining_chunks.next();
            if let Some(chunk_result) = chunk {
                let chunk = match chunk_result {
                    Ok(c) => c,
                    Err(e) => return Some(Err(e)),
                };

                let sender = self.sender.clone();
                let meta = self.shared_meta_data_ref.clone();
                let pedantic = self.pedantic;
                let layer_index = chunk.layer_index;

                self.currently_decompressing_count += 1;

                self.pool.spawn(move || {
                    let result = decompress_deep_chunk(&chunk.compressed_block, &meta, layer_index, pedantic);
                    let _ = sender.send(result);
                });
            } else {
                break;
            }
        }

        if self.currently_decompressing_count > 0 {
            let next = self
                .receiver
                .recv()
                .expect("all decompressing senders hung up");

            self.currently_decompressing_count -= 1;
            Some(next)
        } else {
            None
        }
    }

    /// Access the metadata.
    pub fn meta_data(&self) -> &crate::meta::MetaData {
        self.remaining_chunks.meta_data()
    }
}

#[cfg(feature = "rayon")]
impl<R: super::reader::ChunksReader> ExactSizeIterator for ParallelDeepBlockDecompressor<R> {}

#[cfg(feature = "rayon")]
impl<R: super::reader::ChunksReader> Iterator for ParallelDeepBlockDecompressor<R> {
    type Item = Result<DeepUncompressedBlock>;

    fn next(&mut self) -> Option<Self::Item> {
        self.decompress_next_block()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.remaining_chunks.len() + self.currently_decompressing_count;
        (remaining, Some(remaining))
    }
}

/// Decompress a single compressed chunk into a `DeepUncompressedBlock`.
///
/// Helper function used by both sequential and parallel decompression.
fn decompress_deep_chunk(
    compressed: &crate::block::chunk::CompressedBlock,
    meta: &crate::meta::MetaData,
    layer_index: usize,
    pedantic: bool,
) -> Result<DeepUncompressedBlock> {
    use crate::block::chunk::CompressedBlock;

    let header = &meta.headers[layer_index];

    match compressed {
        CompressedBlock::DeepScanLine(ref block) => {
            let samples = decompress_deep_scanline_block(
                block,
                header.compression,
                &header.channels,
                header.layer_size.width(),
                header.compression.scan_lines_per_block(),
                pedantic,
            )?;

            Ok(DeepUncompressedBlock {
                layer_index,
                y_coordinate: block.y_coordinate,
                samples,
            })
        }
        CompressedBlock::DeepTile(ref block) => {
            // For tiles, tile size comes from header
            let tile_desc = match header.blocks {
                crate::meta::BlockDescription::Tiles(ref tiles) => tiles,
                _ => return Err(Error::invalid("deep tile block in non-tiled layer")),
            };

            let samples = decompress_deep_tile_block(
                block,
                header.compression,
                &header.channels,
                tile_desc.tile_size.width(),
                tile_desc.tile_size.height(),
                pedantic,
            )?;

            Ok(DeepUncompressedBlock {
                layer_index,
                y_coordinate: block.coordinates.tile_index.y() as i32,
                samples,
            })
        }
        _ => Err(Error::invalid("expected deep block, got flat block")),
    }
}

/// Sequential deep block decompressor (fallback when rayon is disabled or unhelpful).
#[derive(Debug)]
pub struct SequentialDeepBlockDecompressor<R: super::reader::ChunksReader> {
    chunks: R,
    pedantic: bool,
}

impl<R: super::reader::ChunksReader> SequentialDeepBlockDecompressor<R> {
    /// Create a new sequential decompressor.
    pub fn new(chunks: R, pedantic: bool) -> Self {
        Self { chunks, pedantic }
    }

    /// Access the metadata.
    pub fn meta_data(&self) -> &crate::meta::MetaData {
        self.chunks.meta_data()
    }
}

impl<R: super::reader::ChunksReader> ExactSizeIterator for SequentialDeepBlockDecompressor<R> {}

impl<R: super::reader::ChunksReader> Iterator for SequentialDeepBlockDecompressor<R> {
    type Item = Result<DeepUncompressedBlock>;

    fn next(&mut self) -> Option<Self::Item> {
        let chunk = self.chunks.next()?;
        let chunk = match chunk {
            Ok(c) => c,
            Err(e) => return Some(Err(e)),
        };

        Some(decompress_deep_chunk(
            &chunk.compressed_block,
            self.chunks.meta_data(),
            chunk.layer_index,
            self.pedantic,
        ))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.chunks.size_hint()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::meta::attribute::ChannelDescription;
    use smallvec::smallvec;

    fn make_test_channels() -> ChannelList {
        ChannelList::new(smallvec![
            ChannelDescription::new("R", SampleType::F32, true),
            ChannelDescription::new("G", SampleType::F32, true),
            ChannelDescription::new("B", SampleType::F32, true),
        ])
    }

    #[test]
    fn roundtrip_deep_scanline_block_uncompressed() {
        let channels = make_test_channels();

        // Create test deep samples 2x2 with [1, 2, 0, 3] samples per pixel
        let mut samples = DeepSamples::new(2, 2);
        samples.set_cumulative_counts(vec![1, 3, 3, 6]).unwrap();
        samples.allocate_channels(&channels);

        // Fill with test data
        for ch in &mut samples.channels {
            if let DeepChannelData::F32(ref mut v) = ch {
                for (i, val) in v.iter_mut().enumerate() {
                    *val = i as f32 * 0.1;
                }
            }
        }

        // Compress
        let block = compress_deep_scanline_block(&samples, Compression::Uncompressed, &channels, 0)
            .unwrap();

        // Decompress
        let recovered = decompress_deep_scanline_block(
            &block,
            Compression::Uncompressed,
            &channels,
            2,
            2,
            true,
        )
        .unwrap();

        assert_eq!(samples.sample_offsets, recovered.sample_offsets);
        assert_eq!(samples.channels.len(), recovered.channels.len());

        for (orig, rec) in samples.channels.iter().zip(recovered.channels.iter()) {
            match (orig, rec) {
                (DeepChannelData::F32(o), DeepChannelData::F32(r)) => {
                    assert_eq!(o, r);
                }
                _ => panic!("channel type mismatch"),
            }
        }
    }

    #[test]
    fn roundtrip_deep_scanline_block_rle() {
        let channels = make_test_channels();

        let mut samples = DeepSamples::new(4, 4);
        samples
            .set_cumulative_counts(vec![1, 1, 2, 3, 3, 4, 5, 5, 6, 6, 6, 7, 8, 9, 10, 12])
            .unwrap();
        samples.allocate_channels(&channels);

        for ch in &mut samples.channels {
            if let DeepChannelData::F32(ref mut v) = ch {
                for (i, val) in v.iter_mut().enumerate() {
                    *val = (i % 10) as f32;
                }
            }
        }

        let block = compress_deep_scanline_block(&samples, Compression::RLE, &channels, 0).unwrap();

        let recovered =
            decompress_deep_scanline_block(&block, Compression::RLE, &channels, 4, 4, true)
                .unwrap();

        assert_eq!(samples.sample_offsets, recovered.sample_offsets);
    }

    #[test]
    fn pack_unpack_deep_channels() {
        let channels = make_test_channels();

        let mut samples = DeepSamples::new(2, 1);
        samples.set_cumulative_counts(vec![2, 5]).unwrap(); // 2 samples, then 3
        samples.allocate_channels(&channels);

        // Set specific values
        if let DeepChannelData::F32(ref mut r) = samples.channels[0] {
            r[0] = 1.0;
            r[1] = 2.0;
            r[2] = 3.0;
            r[3] = 4.0;
            r[4] = 5.0;
        }
        if let DeepChannelData::F32(ref mut g) = samples.channels[1] {
            g[0] = 10.0;
            g[1] = 20.0;
            g[2] = 30.0;
            g[3] = 40.0;
            g[4] = 50.0;
        }
        if let DeepChannelData::F32(ref mut b) = samples.channels[2] {
            b[0] = 100.0;
            b[1] = 200.0;
            b[2] = 300.0;
            b[3] = 400.0;
            b[4] = 500.0;
        }

        let packed = pack_deep_channels(&samples, &channels);

        // Unpack into new samples
        let mut recovered = DeepSamples::new(2, 1);
        recovered.set_cumulative_counts(vec![2, 5]).unwrap();
        unpack_deep_channels(&packed, &mut recovered, &channels).unwrap();

        assert_eq!(samples.channels, recovered.channels);
    }
}
