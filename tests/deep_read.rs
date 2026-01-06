//! Integration tests for reading deep EXR files.

use exr::meta::MetaData;
use exr::block::reader::Reader;
use exr::block::chunk::CompressedBlock;
use exr::block::deep::decompress_deep_scanline_block;
use std::io::BufReader;
use std::fs::File;
use std::path::Path;

/// Test reading metadata from a deep EXR file.
#[test]
fn read_deep_metadata() {
    let path = "tests/images/valid/openexr/v2/LowResLeftView/Balls.exr";
    if !Path::new(path).exists() {
        eprintln!("Skipping test: {} not found", path);
        return;
    }

    let file = BufReader::new(File::open(path).unwrap());
    let meta = MetaData::read_from_buffered(file, false).unwrap();

    println!("Headers: {}", meta.headers.len());
    for (i, header) in meta.headers.iter().enumerate() {
        println!("Header {}: deep={}, compression={:?}", i, header.deep, header.compression);
        println!("  Size: {:?}", header.layer_size);
        println!("  Channels: {}", header.channels.list.len());
        for ch in &header.channels.list {
            println!("    - {} ({:?})", ch.name, ch.sample_type);
        }
        if let Some(version) = header.deep_data_version {
            println!("  Deep data version: {}", version);
        }
        if let Some(max_samples) = header.max_samples_per_pixel {
            println!("  Max samples per pixel: {}", max_samples);
        }
    }

    // Should have at least one header with deep=true
    assert!(meta.headers.iter().any(|h| h.deep), "Expected deep data in Balls.exr");
}

/// Test reading raw deep blocks.
#[test]
fn read_deep_blocks_raw() {
    let path = "tests/images/valid/openexr/v2/LowResLeftView/Balls.exr";
    if !Path::new(path).exists() {
        eprintln!("Skipping test: {} not found", path);
        return;
    }

    let file = BufReader::new(File::open(path).unwrap());
    let reader = Reader::read_from_buffered(file, false).unwrap();
    let meta = reader.meta_data().clone();

    let chunks_reader = reader.all_chunks(false).unwrap();

    let mut deep_block_count = 0;
    let mut total_samples = 0usize;

    for chunk_result in chunks_reader {
        let chunk = chunk_result.unwrap();
        let header = &meta.headers[chunk.layer_index];

        match chunk.compressed_block {
            CompressedBlock::DeepScanLine(ref deep_block) => {
                deep_block_count += 1;

                // Try to decompress
                let samples = decompress_deep_scanline_block(
                    deep_block,
                    header.compression,
                    &header.channels,
                    header.layer_size.width(),
                    header.compression.scan_lines_per_block(),
                    false,
                ).unwrap();

                total_samples += samples.total_samples();

                if deep_block_count == 1 {
                    println!("First deep block:");
                    println!("  y_coordinate: {}", deep_block.y_coordinate);
                    println!("  table size: {}", deep_block.compressed_pixel_offset_table.len());
                    println!("  data size: {}", deep_block.compressed_sample_data_le.len());
                    println!("  decompressed size: {}", deep_block.decompressed_sample_data_size);
                    println!("  DeepSamples:");
                    println!("    width: {}, height: {}", samples.width, samples.height);
                    println!("    total samples: {}", samples.total_samples());
                    println!("    max samples per pixel: {}", samples.max_samples_per_pixel());
                    println!("    channels: {}", samples.channels.len());
                }
            }
            CompressedBlock::DeepTile(ref deep_block) => {
                deep_block_count += 1;
                println!("Found deep tile block at {:?}", deep_block.coordinates);
            }
            _ => {}
        }
    }

    println!("Total deep blocks: {}", deep_block_count);
    println!("Total samples across all blocks: {}", total_samples);

    assert!(deep_block_count > 0, "Expected deep blocks in Balls.exr");
}

/// Test reading all deep test files.
#[test]
fn read_all_deep_test_files() {
    let files = [
        "tests/images/valid/openexr/v2/LowResLeftView/Balls.exr",
        "tests/images/valid/openexr/v2/LowResLeftView/Ground.exr",
        "tests/images/valid/openexr/v2/LowResLeftView/Leaves.exr",
        "tests/images/valid/openexr/v2/LowResLeftView/Trunks.exr",
    ];

    for path in &files {
        if !Path::new(path).exists() {
            eprintln!("Skipping {}: not found", path);
            continue;
        }

        println!("\n=== Testing {} ===", path);

        let file = BufReader::new(File::open(path).unwrap());
        let reader = Reader::read_from_buffered(file, false).unwrap();
        let meta = reader.meta_data().clone();
        let chunks_reader = reader.all_chunks(false).unwrap();

        let mut block_count = 0;
        let mut sample_count = 0usize;

        for chunk_result in chunks_reader {
            let chunk = chunk_result.unwrap();
            let header = &meta.headers[chunk.layer_index];

            if let CompressedBlock::DeepScanLine(ref deep_block) = chunk.compressed_block {
                let samples = decompress_deep_scanline_block(
                    deep_block,
                    header.compression,
                    &header.channels,
                    header.layer_size.width(),
                    header.compression.scan_lines_per_block(),
                    false,
                ).unwrap();

                block_count += 1;
                sample_count += samples.total_samples();
            }
        }

        println!("  Blocks: {}, Total samples: {}", block_count, sample_count);
        assert!(block_count > 0, "Expected deep blocks in {}", path);
    }
}
