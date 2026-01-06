// see https://github.com/openexr/openexr/blob/master/OpenEXR/IlmImf/ImfCompressor.cpp

use super::optimize_bytes::*;
use super::*;
use crate::error::Result;

// scanline decompression routine, see https://github.com/openexr/openexr/blob/master/OpenEXR/IlmImf/ImfScanLineInputFile.cpp
// 1. Uncompress the data, if necessary (If the line is uncompressed, it's in XDR format, regardless of the compressor's output format.)
// 3. Convert one scan line's worth of pixel data back from the machine-independent representation
// 4. Fill the frame buffer with pixel data, respective to sampling and whatnot

/// Raw ZIP decompression without reorder/predict post-processing.
/// Used for deep data sample count tables.
pub fn decompress_zip_raw(compressed: &[u8], expected_size: usize) -> Result<Vec<u8>> {
    let options = zune_inflate::DeflateOptions::default()
        .set_limit(expected_size)
        .set_size_hint(expected_size);
    let mut decoder = zune_inflate::DeflateDecoder::new_with_options(compressed, options);
    decoder
        .decode_zlib()
        .map_err(|_| Error::invalid("zlib-compressed data malformed"))
}

/// Raw ZIP compression without reorder/predict pre-processing.
/// Used for deep data sample count tables.
pub fn compress_zip_raw(data: &[u8]) -> Vec<u8> {
    miniz_oxide::deflate::compress_to_vec_zlib(data, 4)
}

/// Decompress ZIP with full pipeline: ZIP decode -> differences_to_samples -> interleave -> endian convert.
/// Used for flat image data.
pub fn decompress_bytes(
    channels: &ChannelList,
    data_le: ByteVec,
    rectangle: IntegerBounds,
    expected_byte_size: usize,
    _pedantic: bool,
) -> Result<ByteVec> {
    let mut decompressed_le = decompress_zip_raw(&data_le, expected_byte_size)?;

    differences_to_samples(&mut decompressed_le);
    interleave_byte_blocks(&mut decompressed_le);

    super::convert_little_endian_to_current(decompressed_le, channels, rectangle)
}

/// Compress with full pipeline: endian convert -> separate -> samples_to_differences -> ZIP encode.
/// Used for flat image data.
pub fn compress_bytes(
    channels: &ChannelList,
    uncompressed_ne: ByteVec,
    rectangle: IntegerBounds,
) -> Result<ByteVec> {
    // see https://github.com/AcademySoftwareFoundation/openexr/blob/3bd93f85bcb74c77255f28cdbb913fdbfbb39dfe/OpenEXR/IlmImf/ImfTiledOutputFile.cpp#L750-L842
    let mut packed_le = convert_current_to_little_endian(uncompressed_ne, channels, rectangle)?;

    separate_bytes_fragments(&mut packed_le);
    samples_to_differences(&mut packed_le);

    Ok(compress_zip_raw(&packed_le))
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn roundtrip_zip_raw() {
        let data: Vec<u8> = vec![1, 1, 1, 1, 1, 2, 3, 4, 5, 5, 5, 5, 5, 5, 5, 5];
        let compressed = compress_zip_raw(&data);
        let decompressed = decompress_zip_raw(&compressed, data.len()).unwrap();
        assert_eq!(data, decompressed);
    }

    #[test]
    fn roundtrip_zip_raw_empty() {
        // ZIP with empty input still produces a valid zlib stream
        let data: Vec<u8> = vec![];
        let compressed = compress_zip_raw(&data);
        let decompressed = decompress_zip_raw(&compressed, 0).unwrap();
        assert_eq!(data, decompressed);
    }

    #[test]
    fn roundtrip_zip_raw_large() {
        let data: Vec<u8> = (0..10000).map(|i| (i % 256) as u8).collect();
        let compressed = compress_zip_raw(&data);
        let decompressed = decompress_zip_raw(&compressed, data.len()).unwrap();
        assert_eq!(data, decompressed);
    }
}
