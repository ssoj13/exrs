//! Write deep EXR images to files or streams.
//!
//! # Overview
//!
//! This module writes deep images (variable samples per pixel) to OpenEXR files.
//! It provides both high-level image writing and low-level scanline writing.
//!
//! # Writing Pipeline
//!
//! ```text
//! DeepImage / DeepSamples
//!         │
//!         ├── Build Header (deep=true, BlockType=DeepScanLine)
//!         │
//!         ├── For each scanline block:
//!         │    ├── extract_single_line() → line DeepSamples
//!         │    ├── compress_deep_scanline_block()
//!         │    └── write CompressedBlock
//!         │
//!         └── Write offset table
//! ```
//!
//! # Key Design Decisions
//!
//! ## Single-Line Blocks
//!
//! This implementation writes one scanline per block. While OpenEXR supports
//! multi-line blocks (up to `compression.scan_lines_per_block()`), single-line
//! has advantages:
//!
//! - Simpler offset table extraction (no cross-line prefix sum adjustments)
//! - Better streaming support (can write as data becomes available)
//! - Compatibility with all readers
//!
//! The tradeoff is slightly larger file size due to per-block overhead.
//!
//! ## Channel Data Layout
//!
//! The read API stores ALL channel data in the first channel's [`DeepSamples`].
//! This is because deep images use SoA layout where channels share the same
//! sample counts. The writer extracts from this unified storage.
//!
//! # Usage Examples
//!
//! Write a deep image read from another file:
//! ```no_run
//! use exr::image::read::deep::read_first_deep_layer_from_file;
//! use exr::image::write::deep::write_deep_image_to_file;
//! use exr::compression::Compression;
//!
//! let image = read_first_deep_layer_from_file("input.exr")?;
//! write_deep_image_to_file("output.exr", &image, Compression::ZIP1)?;
//! # Ok::<(), exr::error::Error>(())
//! ```
//!
//! Low-level writing with custom samples:
//! ```no_run
//! use exr::image::deep::DeepSamples;
//! use exr::image::write::deep::write_deep_scanlines_to_file;
//! use exr::meta::attribute::ChannelList;
//! use exr::compression::Compression;
//!
//! let samples: DeepSamples = todo!();
//! let channels: ChannelList = todo!();
//! write_deep_scanlines_to_file("output.exr", &samples, &channels, Compression::Uncompressed)?;
//! # Ok::<(), exr::error::Error>(())
//! ```
//!
//! # Compression Support
//!
//! All standard compressions work with deep data:
//! - `Uncompressed` - Fastest, largest files
//! - `ZIP1` - Good balance (recommended)
//! - `RLE` - Fast, moderate compression
//! - `ZIPS` - Best compression, slower
//!
//! # See Also
//!
//! - [`crate::image::read::deep`] - Reading deep images
//! - [`crate::image::deep`] - Core [`DeepSamples`] type
//! - [`crate::block::deep::compress_deep_scanline_block()`] - Block compression

use std::io::{BufWriter, Seek, Write};
use std::path::Path;

use crate::block::chunk::{Chunk, CompressedBlock};
use crate::block::deep::compress_deep_scanline_block;
use crate::block::writer::{ChunkWriter, ChunksWriter};
use crate::compression::Compression;
use crate::error::UnitResult;
use crate::image::deep::DeepSamples;
use crate::image::{AnyChannels, Image, ImageAttributes, Layer, LayerAttributes};
use crate::math::Vec2;
use crate::meta::attribute::{ChannelDescription, ChannelList, IntegerBounds, LineOrder};
use crate::meta::header::{Header, ImageAttributes as HeaderImageAttributes};
use crate::meta::{BlockDescription, Headers, MetaData};

/// Type alias for a deep image with any channels.
pub type DeepImage = Image<Layer<AnyChannels<DeepSamples>>>;

/// Write a deep image to a file.
///
/// # Arguments
/// * `path` - Output file path
/// * `image` - The deep image to write
/// * `compression` - Compression method to use
///
/// # Errors
/// Returns an error if the file cannot be written or data is invalid.
pub fn write_deep_image_to_file(
    path: impl AsRef<Path>,
    image: &DeepImage,
    compression: Compression,
) -> UnitResult {
    crate::io::attempt_delete_file_on_write_error(path.as_ref(), move |write| {
        write_deep_image_to_buffered(BufWriter::new(write), image, compression)
    })
}

/// Write a deep image to a buffered writer.
pub fn write_deep_image_to_buffered<W: Write + Seek>(
    write: W,
    image: &DeepImage,
    compression: Compression,
) -> UnitResult {
    let layer = &image.layer_data;

    // The read API stores ALL channels' data in the FIRST channel's DeepSamples
    let first_ch = &layer.channel_data.list[0];
    let samples = &first_ch.sample_data;

    // Build channel list - channel data order matches list order
    let channel_list = ChannelList::new(
        layer
            .channel_data
            .list
            .iter()
            .enumerate()
            .map(|(idx, ch)| ChannelDescription {
                name: ch.name.clone(),
                sample_type: samples
                    .channels
                    .get(idx)
                    .map(|c| c.sample_type())
                    .unwrap_or(crate::meta::attribute::SampleType::F32),
                quantize_linearly: ch.quantize_linearly,
                sampling: ch.sampling,
            })
            .collect(),
    );

    write_deep_scanlines_to_buffered(
        write,
        samples,
        &channel_list,
        compression,
        Some(&image.attributes),
        Some(&layer.attributes),
    )
}

/// Write deep scanline data to a file.
///
/// Lower-level function that writes a single DeepSamples block.
pub fn write_deep_scanlines_to_file(
    path: impl AsRef<Path>,
    samples: &DeepSamples,
    channels: &ChannelList,
    compression: Compression,
) -> UnitResult {
    crate::io::attempt_delete_file_on_write_error(path.as_ref(), move |write| {
        write_deep_scanlines_to_buffered(
            BufWriter::new(write),
            samples,
            channels,
            compression,
            None,
            None,
        )
    })
}

/// Write deep scanline data to a buffered writer.
fn write_deep_scanlines_to_buffered<W: Write + Seek>(
    write: W,
    samples: &DeepSamples,
    channels: &ChannelList,
    compression: Compression,
    image_attrs: Option<&ImageAttributes>,
    layer_attrs: Option<&LayerAttributes>,
) -> UnitResult {
    let width = samples.width;
    let height = samples.height;
    let data_size = Vec2(width, height);

    // Calculate max samples per pixel for header
    let max_samples = samples.max_samples_per_pixel();

    // Build header for deep scanline data
    let header = Header {
        channels: channels.clone(),
        compression,
        blocks: BlockDescription::ScanLines,
        line_order: LineOrder::Increasing,
        layer_size: data_size,
        shared_attributes: image_attrs
            .map(|a| HeaderImageAttributes {
                pixel_aspect: a.pixel_aspect,
                chromaticities: a.chromaticities.clone(),
                time_code: a.time_code,
                display_window: a.display_window,
                other: a.other.clone(),
            })
            .unwrap_or_else(|| HeaderImageAttributes {
                pixel_aspect: 1.0,
                chromaticities: None,
                time_code: None,
                display_window: IntegerBounds::new((0, 0), data_size),
                other: Default::default(),
            }),
        own_attributes: {
            let mut attrs = layer_attrs.cloned().unwrap_or_default();
            // Deep files require a layer name
            if attrs.layer_name.is_none() {
                attrs.layer_name = Some(crate::meta::attribute::Text::new_or_panic("deep"));
            }
            attrs
        },

        // Deep data specific
        deep: true,
        deep_data_version: Some(1),
        max_samples_per_pixel: Some(max_samples as usize),

        // Multi-part
        chunk_count: calculate_chunk_count(height, compression),
    };

    let headers: Headers = smallvec::smallvec![header];

    // Write the file
    crate::block::writer::write_chunks_with(write, headers, true, |meta, chunk_writer| {
        write_deep_chunks(chunk_writer, &meta, samples, channels, compression)
    })
}

/// Calculate chunk count for deep scanlines.
fn calculate_chunk_count(height: usize, compression: Compression) -> usize {
    let lines_per_block = compression.scan_lines_per_block();
    (height + lines_per_block - 1) / lines_per_block
}

/// Write deep scanline chunks to the writer.
fn write_deep_chunks<W: Write + Seek>(
    writer: &mut ChunkWriter<W>,
    meta: &MetaData,
    samples: &DeepSamples,
    channels: &ChannelList,
    compression: Compression,
) -> UnitResult {
    let header = &meta.headers[0];
    let height = header.layer_size.height();
    let lines_per_block = compression.scan_lines_per_block();

    let mut block_idx = 0;
    let mut y = 0;

    while y < height {
        let block_height = lines_per_block.min(height - y);

        // Extract samples for this block
        let block_samples = extract_block_samples(samples, y, block_height, channels);

        // Compress to deep scanline block
        let compressed =
            compress_deep_scanline_block(&block_samples, compression, channels, y as i32)?;

        let chunk = Chunk {
            layer_index: 0,
            compressed_block: CompressedBlock::DeepScanLine(compressed),
        };

        writer.write_chunk(block_idx, chunk)?;

        block_idx += 1;
        y += block_height;
    }

    Ok(())
}

/// Extract a subset of samples for a specific block (y range).
fn extract_block_samples(
    samples: &DeepSamples,
    y_start: usize,
    block_height: usize,
    channels: &ChannelList,
) -> DeepSamples {
    let width = samples.width;

    // For single-line blocks (common case), optimize
    if block_height == 1 {
        return extract_single_line(samples, y_start, channels);
    }

    // Multi-line block extraction
    let mut block = DeepSamples::new(width, block_height);

    // Calculate cumulative counts for the block
    let mut cumulative: Vec<u32> = Vec::with_capacity(width * block_height);
    let mut total = 0u32;

    for y in y_start..(y_start + block_height) {
        for x in 0..width {
            let count = samples.sample_count(x, y);
            total += count as u32;
            cumulative.push(total);
        }
    }

    block
        .set_cumulative_counts(cumulative)
        .expect("valid cumulative counts");
    // Allocate channels with same types as source
    allocate_channels_like(&mut block, samples);

    // Copy sample data
    let total_samples = total as usize;
    if total_samples == 0 {
        return block;
    }

    for ch_idx in 0..samples.channels.len() {
        let src_data = &samples.channels[ch_idx];
        let dst_data = &mut block.channels[ch_idx];

        let mut dst_idx = 0;
        for y in y_start..(y_start + block_height) {
            for x in 0..width {
                let count = samples.sample_count(x, y);
                if count > 0 {
                    let (src_start, _) = samples.sample_range(y * width + x);
                    copy_channel_samples(src_data, dst_data, src_start, dst_idx, count);
                    dst_idx += count;
                }
            }
        }
    }

    block
}

/// Extract a single scanline of samples from the full image.
///
/// # Algorithm
///
/// 1. Compute line's pixel range: `[y * width, (y+1) * width)`
/// 2. Find `line_start_offset` = cumulative samples before this line
/// 3. Build line-local cumulative counts by subtracting `line_start_offset`
/// 4. Copy sample data from source to new [`DeepSamples`]
///
/// # Offset Adjustment
///
/// The full image has global cumulative offsets:
/// ```text
/// Line 0: [a, b, c]       -> line-local: [a, b, c]
/// Line 1: [a+x, b+x, c+x] -> line-local: [a, b, c] (subtract x)
/// ```
///
/// This is necessary because each line block is written independently and
/// readers expect each block's offset table to start from 0.
///
/// # Arguments
///
/// * `samples` - Full image sample data
/// * `y` - Scanline index (0-based)
/// * `_channels` - Channel list (unused, kept for API symmetry)
fn extract_single_line(samples: &DeepSamples, y: usize, _channels: &ChannelList) -> DeepSamples {
    let width = samples.width;
    let mut block = DeepSamples::new(width, 1);

    // Get cumulative counts for this line
    let start_pixel = y * width;
    let end_pixel = start_pixel + width;

    // Calculate line-local cumulative counts
    let line_start_offset = if start_pixel == 0 {
        0
    } else {
        samples.sample_offsets[start_pixel - 1]
    };

    let cumulative: Vec<u32> = samples.sample_offsets[start_pixel..end_pixel]
        .iter()
        .map(|&v| v.saturating_sub(line_start_offset))
        .collect();

    let total_samples = *cumulative.last().unwrap_or(&0) as usize;

    block
        .set_cumulative_counts(cumulative)
        .expect("valid cumulative counts");
    // Allocate channels with same types as source
    allocate_channels_like(&mut block, samples);

    if total_samples == 0 {
        return block;
    }

    // Copy sample data
    let src_start = line_start_offset as usize;

    for ch_idx in 0..samples.channels.len() {
        let src_data = &samples.channels[ch_idx];
        let dst_data = &mut block.channels[ch_idx];

        copy_channel_samples(src_data, dst_data, src_start, 0, total_samples);
    }

    block
}

/// Allocate channels in destination with same types as source.
fn allocate_channels_like(dest: &mut DeepSamples, source: &DeepSamples) {
    use crate::image::deep::DeepChannelData;

    let total = dest.total_samples();
    dest.channels.clear();

    for src_ch in &source.channels {
        let new_ch = match src_ch {
            DeepChannelData::F16(_) => DeepChannelData::F16(vec![half::f16::ZERO; total]),
            DeepChannelData::F32(_) => DeepChannelData::F32(vec![0.0; total]),
            DeepChannelData::U32(_) => DeepChannelData::U32(vec![0; total]),
        };
        dest.channels.push(new_ch);
    }
}

/// Copy samples from one channel to another.
fn copy_channel_samples(
    src: &crate::image::deep::DeepChannelData,
    dst: &mut crate::image::deep::DeepChannelData,
    src_start: usize,
    dst_start: usize,
    count: usize,
) {
    use crate::image::deep::DeepChannelData;

    match (src, dst) {
        (DeepChannelData::F16(s), DeepChannelData::F16(d)) => {
            d[dst_start..dst_start + count].copy_from_slice(&s[src_start..src_start + count]);
        }
        (DeepChannelData::F32(s), DeepChannelData::F32(d)) => {
            d[dst_start..dst_start + count].copy_from_slice(&s[src_start..src_start + count]);
        }
        (DeepChannelData::U32(s), DeepChannelData::U32(d)) => {
            d[dst_start..dst_start + count].copy_from_slice(&s[src_start..src_start + count]);
        }
        _ => panic!("channel type mismatch"),
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::meta::attribute::SampleType;

    #[test]
    fn test_write_simple_deep() {
        let mut samples = DeepSamples::new(4, 4);

        // Set up some sample counts (cumulative)
        // 1 sample, 2 samples, 0 samples, 1 sample per pixel in first row
        samples
            .set_cumulative_counts(vec![
                1, 3, 3, 4, // row 0
                5, 5, 6, 7, // row 1
                8, 9, 10, 11, // row 2
                12, 13, 14, 16, // row 3
            ])
            .unwrap();

        let channels = ChannelList::new(smallvec::smallvec![
            ChannelDescription::named("B", SampleType::F32),
            ChannelDescription::named("G", SampleType::F32),
            ChannelDescription::named("R", SampleType::F32),
        ]);

        samples.allocate_channels(&channels);

        // Fill with some test data
        for ch in &mut samples.channels {
            if let crate::image::deep::DeepChannelData::F32(ref mut v) = ch {
                for (i, val) in v.iter_mut().enumerate() {
                    *val = i as f32 * 0.1;
                }
            }
        }

        // Write to a buffer
        let mut buffer = std::io::Cursor::new(Vec::new());
        write_deep_scanlines_to_buffered(
            &mut buffer,
            &samples,
            &channels,
            Compression::Uncompressed,
            None,
            None,
        )
        .expect("write should succeed");

        // Verify buffer has data
        assert!(buffer.get_ref().len() > 0, "output should have data");
    }

    #[test]
    fn test_extract_single_line() {
        let mut samples = DeepSamples::new(3, 2);
        samples
            .set_cumulative_counts(vec![
                2, 5, 5, // row 0: 2, 3, 0 samples
                6, 8, 10, // row 1: 1, 2, 2 samples
            ])
            .unwrap();

        let channels = ChannelList::new(smallvec::smallvec![ChannelDescription::named(
            "A",
            SampleType::F32
        ),]);
        samples.allocate_channels(&channels);

        // Fill data
        if let crate::image::deep::DeepChannelData::F32(ref mut v) = samples.channels[0] {
            for (i, val) in v.iter_mut().enumerate() {
                *val = i as f32;
            }
        }

        // Extract line 1
        let line1 = extract_single_line(&samples, 1, &channels);

        assert_eq!(line1.width, 3);
        assert_eq!(line1.height, 1);
        assert_eq!(line1.total_samples(), 5); // 1 + 2 + 2

        // Verify sample counts
        assert_eq!(line1.sample_count(0, 0), 1);
        assert_eq!(line1.sample_count(1, 0), 2);
        assert_eq!(line1.sample_count(2, 0), 2);
    }

    #[test]
    fn test_roundtrip_deep_file() {
        use crate::image::read::deep::read_first_deep_layer_from_file;
        use std::path::Path;

        let path = "tests/images/valid/openexr/v2/LowResLeftView/Leaves.exr";
        if !Path::new(path).exists() {
            eprintln!("Skipping roundtrip test: {} not found", path);
            return;
        }

        // Read original file
        let original = read_first_deep_layer_from_file(path).expect("failed to read original");

        // Write to temp buffer
        let mut buffer = std::io::Cursor::new(Vec::new());
        write_deep_image_to_buffered(&mut buffer, &original, Compression::Uncompressed)
            .expect("failed to write");

        // Read back
        buffer.set_position(0);
        let roundtrip = crate::image::read::deep::read_deep()
            .all_channels()
            .first_valid_layer()
            .all_attributes()
            .from_buffered(std::io::BufReader::new(buffer))
            .expect("failed to read back");

        // Compare dimensions and totals
        let orig_samples = &original.layer_data.channel_data.list[0].sample_data;
        let rt_samples = &roundtrip.layer_data.channel_data.list[0].sample_data;

        assert_eq!(orig_samples.width, rt_samples.width, "width mismatch");
        assert_eq!(orig_samples.height, rt_samples.height, "height mismatch");
        assert_eq!(
            orig_samples.total_samples(),
            rt_samples.total_samples(),
            "total samples mismatch"
        );

        // Verify sample counts match
        for y in 0..orig_samples.height {
            for x in 0..orig_samples.width {
                assert_eq!(
                    orig_samples.sample_count(x, y),
                    rt_samples.sample_count(x, y),
                    "sample count mismatch at ({}, {})",
                    x,
                    y
                );
            }
        }

        // Verify channel data matches
        if !orig_samples.channels.is_empty() && !rt_samples.channels.is_empty() {
            use crate::image::deep::DeepChannelData;

            match (&orig_samples.channels[0], &rt_samples.channels[0]) {
                (DeepChannelData::F32(orig), DeepChannelData::F32(rt)) => {
                    assert_eq!(orig.len(), rt.len(), "f32 data length mismatch");
                    for (i, (o, r)) in orig.iter().zip(rt.iter()).enumerate() {
                        assert!(
                            (o - r).abs() < 1e-6,
                            "f32 value mismatch at {}: {} vs {}",
                            i,
                            o,
                            r
                        );
                    }
                }
                (DeepChannelData::F16(orig), DeepChannelData::F16(rt)) => {
                    assert_eq!(orig.len(), rt.len(), "f16 data length mismatch");
                    for (i, (o, r)) in orig.iter().zip(rt.iter()).enumerate() {
                        assert_eq!(o.to_bits(), r.to_bits(), "f16 value mismatch at {}", i);
                    }
                }
                (DeepChannelData::U32(orig), DeepChannelData::U32(rt)) => {
                    assert_eq!(orig, rt, "u32 data mismatch");
                }
                _ => panic!("channel type mismatch"),
            }
        }
    }
}
