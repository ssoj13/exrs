use super::optimize_bytes::*;
use super::Error;
use super::Result;
use super::*;

// inspired by  https://github.com/openexr/openexr/blob/master/OpenEXR/IlmImf/ImfRle.cpp

const MIN_RUN_LENGTH: usize = 3;
const MAX_RUN_LENGTH: usize = 127;

/// Raw RLE decompression without reorder/predict post-processing.
/// Used for deep data sample count tables.
pub fn decompress_rle_raw(
    compressed: &[u8],
    expected_size: usize,
    pedantic: bool,
) -> Result<Vec<u8>> {
    let mut remaining = compressed;
    let mut decompressed = Vec::with_capacity(expected_size.min(8 * 2048));

    while !remaining.is_empty() && decompressed.len() != expected_size {
        let count = take_1(&mut remaining)? as i8 as i32;

        if count < 0 {
            // take the next '-count' bytes as-is
            let values = take_n(&mut remaining, (-count) as usize)?;
            decompressed.extend_from_slice(values);
        } else {
            // repeat the next value 'count + 1' times
            let value = take_1(&mut remaining)?;
            decompressed.resize(decompressed.len() + count as usize + 1, value);
        }
    }

    if pedantic && !remaining.is_empty() {
        return Err(Error::invalid("data amount"));
    }

    Ok(decompressed)
}

/// Raw RLE compression without reorder/predict pre-processing.
/// Used for deep data sample count tables.
pub fn compress_rle_raw(data: &[u8]) -> Vec<u8> {
    let mut compressed = Vec::with_capacity(data.len());
    let mut run_start = 0;
    let mut run_end = 1;

    while run_start < data.len() {
        while run_end < data.len()
            && data[run_start] == data[run_end]
            && (run_end - run_start) as i32 - 1 < MAX_RUN_LENGTH as i32
        {
            run_end += 1;
        }

        if run_end - run_start >= MIN_RUN_LENGTH {
            compressed.push(((run_end - run_start) as i32 - 1) as u8);
            compressed.push(data[run_start]);
            run_start = run_end;
        } else {
            while run_end < data.len()
                && ((run_end + 1 >= data.len() || data[run_end] != data[run_end + 1])
                    || (run_end + 2 >= data.len() || data[run_end + 1] != data[run_end + 2]))
                && run_end - run_start < MAX_RUN_LENGTH
            {
                run_end += 1;
            }

            compressed.push((run_start as i32 - run_end as i32) as u8);
            compressed.extend_from_slice(&data[run_start..run_end]);

            run_start = run_end;
            run_end += 1;
        }
    }

    compressed
}

/// Decompress RLE with full pipeline: RLE decode -> differences_to_samples -> interleave -> endian convert.
/// Used for flat image data.
pub fn decompress_bytes(
    channels: &ChannelList,
    compressed_le: ByteVec,
    rectangle: IntegerBounds,
    expected_byte_size: usize,
    pedantic: bool,
) -> Result<ByteVec> {
    let mut decompressed_le = decompress_rle_raw(&compressed_le, expected_byte_size, pedantic)?;

    differences_to_samples(&mut decompressed_le);
    interleave_byte_blocks(&mut decompressed_le);
    super::convert_little_endian_to_current(decompressed_le, channels, rectangle)
}

/// Compress with full pipeline: endian convert -> separate -> samples_to_differences -> RLE encode.
/// Used for flat image data.
pub fn compress_bytes(
    channels: &ChannelList,
    uncompressed_ne: ByteVec,
    rectangle: IntegerBounds,
) -> Result<ByteVec> {
    // see https://github.com/AcademySoftwareFoundation/openexr/blob/3bd93f85bcb74c77255f28cdbb913fdbfbb39dfe/OpenEXR/IlmImf/ImfTiledOutputFile.cpp#L750-L842
    let mut data_le =
        super::convert_current_to_little_endian(uncompressed_ne, channels, rectangle)?;

    separate_bytes_fragments(&mut data_le);
    samples_to_differences(&mut data_le);

    Ok(compress_rle_raw(&data_le))
}

fn take_1(slice: &mut &[u8]) -> Result<u8> {
    if !slice.is_empty() {
        let result = slice[0];
        *slice = &slice[1..];
        Ok(result)
    } else {
        Err(Error::invalid("compressed data"))
    }
}

fn take_n<'s>(slice: &mut &'s [u8], n: usize) -> Result<&'s [u8]> {
    if n <= slice.len() {
        let (front, back) = slice.split_at(n);
        *slice = back;
        Ok(front)
    } else {
        Err(Error::invalid("compressed data"))
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn roundtrip_rle_raw() {
        let data: Vec<u8> = vec![1, 1, 1, 1, 1, 2, 3, 4, 5, 5, 5, 5, 5, 5, 5, 5];
        let compressed = compress_rle_raw(&data);
        let decompressed = decompress_rle_raw(&compressed, data.len(), true).unwrap();
        assert_eq!(data, decompressed);
    }

    #[test]
    fn roundtrip_rle_raw_empty() {
        let data: Vec<u8> = vec![];
        let compressed = compress_rle_raw(&data);
        let decompressed = decompress_rle_raw(&compressed, 0, true).unwrap();
        assert_eq!(data, decompressed);
    }

    #[test]
    fn roundtrip_rle_raw_single() {
        let data: Vec<u8> = vec![42];
        let compressed = compress_rle_raw(&data);
        let decompressed = decompress_rle_raw(&compressed, 1, true).unwrap();
        assert_eq!(data, decompressed);
    }

    #[test]
    fn roundtrip_rle_raw_no_runs() {
        let data: Vec<u8> = vec![1, 2, 3, 4, 5, 6, 7, 8];
        let compressed = compress_rle_raw(&data);
        let decompressed = decompress_rle_raw(&compressed, data.len(), true).unwrap();
        assert_eq!(data, decompressed);
    }

    #[test]
    fn roundtrip_rle_raw_all_same() {
        let data: Vec<u8> = vec![7; 200];
        let compressed = compress_rle_raw(&data);
        let decompressed = decompress_rle_raw(&compressed, data.len(), true).unwrap();
        assert_eq!(data, decompressed);
        // Should compress well
        assert!(compressed.len() < data.len());
    }
}
