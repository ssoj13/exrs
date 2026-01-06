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
use crate::compression::{Compression, deep as deep_compress};
use crate::error::{Error, Result};
use crate::image::deep::{DeepSamples, DeepChannelData};
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
    let table_bytes: Vec<u8> = block.compressed_pixel_offset_table
        .iter()
        .map(|&b| b as u8)
        .collect();

    let cumulative_counts = deep_compress::decompress_sample_table(
        compression,
        &table_bytes,
        width,
        height,
        pedantic,
    )?;

    // Validate counts
    deep_compress::validate_sample_table(&cumulative_counts)?;

    // Create DeepSamples structure
    let mut samples = DeepSamples::new(width, height);

    // Convert i32 cumulative to u32
    let cumulative_u32: Vec<u32> = cumulative_counts
        .iter()
        .map(|&c| c as u32)
        .collect();

    samples.set_cumulative_counts(cumulative_u32)?;

    // Decompress sample data
    let decompressed_data = deep_compress::decompress_sample_data(
        compression,
        &block.compressed_sample_data_le,
        block.decompressed_sample_data_size,
        pedantic,
    )?;

    // Unpack channel data
    unpack_deep_channels(
        &decompressed_data,
        &mut samples,
        channels,
    )?;

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
    let table_bytes: Vec<u8> = block.compressed_pixel_offset_table
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

    let cumulative_u32: Vec<u32> = cumulative_counts
        .iter()
        .map(|&c| c as u32)
        .collect();

    samples.set_cumulative_counts(cumulative_u32)?;

    // Decompress sample data
    let decompressed_data = deep_compress::decompress_sample_data(
        compression,
        &block.compressed_sample_data_le,
        block.decompressed_sample_data_size,
        pedantic,
    )?;

    // Unpack channel data
    unpack_deep_channels(
        &decompressed_data,
        &mut samples,
        channels,
    )?;

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
    let bytes_per_sample: usize = channels.list.iter()
        .map(|ch| ch.sample_type.bytes_per_sample())
        .sum();

    let expected_size = total_samples * bytes_per_sample;
    if data.len() != expected_size {
        return Err(Error::invalid(format!(
            "deep sample data size mismatch: got {}, expected {} ({} samples * {} bytes)",
            data.len(), expected_size, total_samples, bytes_per_sample
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
pub fn pack_deep_channels(
    samples: &DeepSamples,
    channels: &ChannelList,
) -> Vec<u8> {
    let total_samples = samples.total_samples();

    if total_samples == 0 {
        return Vec::new();
    }

    let bytes_per_sample: usize = channels.list.iter()
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
    let cumulative_i32: Vec<i32> = samples.sample_offsets
        .iter()
        .map(|&c| c as i32)
        .collect();

    // Compress sample count table
    let compressed_table = deep_compress::compress_sample_table(
        compression,
        &cumulative_i32,
    )?;

    // Pack and compress sample data
    let packed_data = pack_deep_channels(samples, channels);
    let decompressed_size = packed_data.len();

    let compressed_data = deep_compress::compress_sample_data(
        compression,
        &packed_data,
    )?;

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
    let cumulative_i32: Vec<i32> = samples.sample_offsets
        .iter()
        .map(|&c| c as i32)
        .collect();

    // Compress sample count table
    let compressed_table = deep_compress::compress_sample_table(
        compression,
        &cumulative_i32,
    )?;

    // Pack and compress sample data
    let packed_data = pack_deep_channels(samples, channels);
    let decompressed_size = packed_data.len();

    let compressed_data = deep_compress::compress_sample_data(
        compression,
        &packed_data,
    )?;

    Ok(CompressedDeepTileBlock {
        coordinates,
        decompressed_sample_data_size: decompressed_size,
        compressed_pixel_offset_table: compressed_table.iter().map(|&b| b as i8).collect(),
        compressed_sample_data_le: compressed_data,
    })
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
        let block = compress_deep_scanline_block(
            &samples,
            Compression::Uncompressed,
            &channels,
            0,
        ).unwrap();

        // Decompress
        let recovered = decompress_deep_scanline_block(
            &block,
            Compression::Uncompressed,
            &channels,
            2, 2, true,
        ).unwrap();

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
        samples.set_cumulative_counts(vec![
            1, 1, 2, 3,
            3, 4, 5, 5,
            6, 6, 6, 7,
            8, 9, 10, 12,
        ]).unwrap();
        samples.allocate_channels(&channels);

        for ch in &mut samples.channels {
            if let DeepChannelData::F32(ref mut v) = ch {
                for (i, val) in v.iter_mut().enumerate() {
                    *val = (i % 10) as f32;
                }
            }
        }

        let block = compress_deep_scanline_block(
            &samples,
            Compression::RLE,
            &channels,
            0,
        ).unwrap();

        let recovered = decompress_deep_scanline_block(
            &block,
            Compression::RLE,
            &channels,
            4, 4, true,
        ).unwrap();

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
            r[0] = 1.0; r[1] = 2.0; r[2] = 3.0; r[3] = 4.0; r[4] = 5.0;
        }
        if let DeepChannelData::F32(ref mut g) = samples.channels[1] {
            g[0] = 10.0; g[1] = 20.0; g[2] = 30.0; g[3] = 40.0; g[4] = 50.0;
        }
        if let DeepChannelData::F32(ref mut b) = samples.channels[2] {
            b[0] = 100.0; b[1] = 200.0; b[2] = 300.0; b[3] = 400.0; b[4] = 500.0;
        }

        let packed = pack_deep_channels(&samples, &channels);

        // Unpack into new samples
        let mut recovered = DeepSamples::new(2, 1);
        recovered.set_cumulative_counts(vec![2, 5]).unwrap();
        unpack_deep_channels(&packed, &mut recovered, &channels).unwrap();

        assert_eq!(samples.channels, recovered.channels);
    }
}
