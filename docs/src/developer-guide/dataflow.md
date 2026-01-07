# Data Flow

How data moves through exrs when reading and writing EXR files.

## Reading Pipeline

### Overview

```
File/Bytes → Metadata → Chunks → Decompress → Blocks → Lines → Image
```

### Detailed Flow

```
┌──────────────────────────────────────────────────────────────────────┐
│                           USER CODE                                   │
│  read().no_deep_data().all_channels().first_valid_layer()...         │
└──────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌──────────────────────────────────────────────────────────────────────┐
│                         image/read/mod.rs                             │
│                                                                       │
│  ReadBuilder → ReadFlatSamples → ReadChannels → ReadLayers → Read    │
└──────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌──────────────────────────────────────────────────────────────────────┐
│                         block/reader.rs                               │
│                                                                       │
│  Reader::read_from_buffered(read, pedantic)                          │
│    1. Validate magic number (0x762f3101)                             │
│    2. Read Requirements (version, flags)                              │
│    3. Read Headers (one per layer)                                    │
│    4. Read/Skip OffsetTables                                          │
│                                                                       │
│  reader.all_chunks(pedantic) → ChunksReader                          │
│    - Iterator over Chunk (compressed blocks)                          │
│    - Validates offsets                                                │
└──────────────────────────────────────────────────────────────────────┘
                                    │
                    ┌───────────────┴───────────────┐
                    ▼                               ▼
            ┌─────────────┐                 ┌─────────────┐
            │ Sequential  │                 │  Parallel   │
            │   Reading   │                 │  (rayon)    │
            └─────────────┘                 └─────────────┘
                    │                               │
                    └───────────────┬───────────────┘
                                    ▼
┌──────────────────────────────────────────────────────────────────────┐
│                       compression/mod.rs                              │
│                                                                       │
│  Compression::decompress_image_section_from_le()                     │
│                                                                       │
│    match compression {                                                │
│      Uncompressed => convert endianness                               │
│      RLE          => rle::decompress()                                │
│      ZIP/ZIPS     => zip::decompress()                                │
│      PIZ          => piz::decompress()                                │
│      PXR24        => pxr24::decompress()                              │
│      B44/B44A     => b44::decompress()                                │
│    }                                                                  │
│                                                                       │
│  Post-processing:                                                     │
│    differences_to_samples() - undo delta encoding                     │
│    interleave_byte_blocks() - reorder bytes                           │
│    little-endian → native-endian                                      │
└──────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌──────────────────────────────────────────────────────────────────────┐
│                          block/mod.rs                                 │
│                                                                       │
│  UncompressedBlock::decompress_chunk(chunk, meta, pedantic)          │
│    Returns: UncompressedBlock { index, data: Vec<u8> }               │
│                                                                       │
│  block.lines(channels) → Iterator<LineRef>                           │
│    Yields lines for each channel, each scan line                      │
└──────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌──────────────────────────────────────────────────────────────────────┐
│                 image/read/specific_channels.rs                       │
│                                                                       │
│  SpecificChannelsReader::filter_block()                              │
│    - Reads line samples into user storage                             │
│    - Calls user's set_pixel() callback                                │
│    - Converts sample types as needed                                  │
└──────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
                          ┌───────────────────┐
                          │  Image<Layers>    │
                          │  (User receives)  │
                          └───────────────────┘
```

## Writing Pipeline

### Overview

```
Image → Blocks → Lines → Compress → Chunks → Metadata → File
```

### Detailed Flow

```
┌──────────────────────────────────────────────────────────────────────┐
│                           USER CODE                                   │
│  let image = Image::from_layer(layer);                               │
│  image.write().to_file("output.exr")?;                               │
└──────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌──────────────────────────────────────────────────────────────────────┐
│                        image/write/mod.rs                             │
│                                                                       │
│  WriteImageWithOptions::to_buffered(write)                           │
│    1. headers = self.infer_meta_data()                               │
│    2. layers = self.image.create_writer(&headers)                    │
│    3. block::write(write, headers, checks, |meta, writer| { ... })   │
└──────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌──────────────────────────────────────────────────────────────────────┐
│                         block/writer.rs                               │
│                                                                       │
│  block::write(write, headers, checks, callback)                      │
│    1. MetaData::write_validating_to_buffered() - magic + headers     │
│    2. Write placeholder offset table                                  │
│    3. Create ChunksWriter                                            │
│    4. Call user callback to write chunks                              │
│    5. Seek back and write final offset table                          │
└──────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌──────────────────────────────────────────────────────────────────────┐
│                     ChunksWriter (block/writer.rs)                    │
│                                                                       │
│  compress_all_blocks_parallel()  - rayon thread pool                  │
│  compress_all_blocks_sequential() - single thread                     │
│                                                                       │
│  For each block:                                                      │
│    LayersWriter::extract_uncompressed_block()                        │
│    UncompressedBlock::compress_to_chunk()                            │
│    Write chunk to file                                                │
└──────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌──────────────────────────────────────────────────────────────────────┐
│                      image/write/channels.rs                          │
│                                                                       │
│  ChannelsWriter::extract_uncompressed_block()                        │
│    - Iterates channels                                                │
│    - Calls user's get_pixel()                                        │
│    - Builds UncompressedBlock::from_lines()                          │
└──────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌──────────────────────────────────────────────────────────────────────┐
│                          block/mod.rs                                 │
│                                                                       │
│  UncompressedBlock::compress_to_chunk(headers)                       │
│    1. Get header for this block's layer                               │
│    2. header.compression.compress_image_section_to_le()              │
│    3. Return Chunk { layer_index, compressed_block }                 │
└──────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌──────────────────────────────────────────────────────────────────────┐
│                       compression/mod.rs                              │
│                                                                       │
│  Compression::compress_image_section_to_le()                         │
│                                                                       │
│  Pre-processing:                                                      │
│    native-endian → little-endian                                      │
│    separate_bytes_fragments() - reorder bytes                         │
│    samples_to_differences() - delta encoding                          │
│                                                                       │
│  match compression {                                                  │
│    Uncompressed => return LE bytes                                    │
│    RLE          => rle::compress()                                    │
│    ZIP/ZIPS     => zip::compress()                                    │
│    PIZ          => piz::compress()                                    │
│    PXR24        => pxr24::compress()                                  │
│    B44/B44A     => b44::compress()                                    │
│  }                                                                    │
└──────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
                          ┌───────────────────┐
                          │   .exr File       │
                          │   (Written)       │
                          └───────────────────┘
```

## Deep Data Pipeline

### Reading Deep

```
┌──────────────────────────────────────────────────────────────────────┐
│                       Deep Data Reading                               │
│                                                                       │
│  read_deep().all_channels().first_valid_layer().from_file()          │
└──────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌──────────────────────────────────────────────────────────────────────┐
│                      image/read/deep.rs                               │
│                                                                       │
│  ReadDeepImage::from_buffered()                                      │
│    1. Reader::read_from_buffered()                                   │
│    2. Find deep layers (header.deep == true)                         │
│    3. read_deep_layer_internal()                                     │
│       a. reader.all_chunks()                                         │
│       b. decompress_blocks_parallel/sequential()                     │
│       c. Sort blocks by y-coordinate                                  │
│       d. merge_deep_blocks() - combine into DeepSamples              │
│    4. build_deep_layer()                                             │
└──────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌──────────────────────────────────────────────────────────────────────┐
│                        block/deep.rs                                  │
│                                                                       │
│  decompress_deep_scanline_block()                                    │
│    1. Decompress sample count table (ZIP)                            │
│    2. Validate cumulative counts                                      │
│    3. Decompress sample data per compression                          │
│    4. Unpack channel data into DeepSamples                           │
└──────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌──────────────────────────────────────────────────────────────────────┐
│                        image/deep.rs                                  │
│                                                                       │
│  DeepSamples {                                                       │
│    sample_offsets: Vec<u32>,    // Cumulative counts                 │
│    channels: Vec<DeepChannelData>,                                   │
│    width, height                                                      │
│  }                                                                    │
└──────────────────────────────────────────────────────────────────────┘
```

### Block Merging

```
┌─────────────────────────────────────────────────────────────────────┐
│                     merge_deep_blocks()                              │
│                                                                      │
│  Input: Vec<(y_offset, DeepSamples)>  // Per-block samples          │
│                                                                      │
│  1. Build offset table                                               │
│     combined_offsets[total_pixels + 1] with leading 0               │
│                                                                      │
│  2. Collect counts                                                   │
│     Copy per-pixel counts to correct image positions                 │
│                                                                      │
│  3. Prefix sum                                                       │
│     Convert individual → cumulative offsets                          │
│                                                                      │
│  4. Allocate channels                                                │
│     Create output arrays sized to total_samples                      │
│                                                                      │
│  5. Copy data                                                        │
│     Place each block's samples at correct positions                  │
│                                                                      │
│  Output: DeepSamples for full image                                  │
└─────────────────────────────────────────────────────────────────────┘
```

## Compression Flow

### Compression Pre-processing

```
Native Endian Bytes (from user)
         │
         ▼
┌─────────────────────────────────┐
│  Convert to Little Endian       │
│  (sample_to_le for each type)   │
└─────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────┐
│  separate_bytes_fragments()     │
│  Reorder: [AABBCC] → [ABCABC]   │
│  (improves compression ratio)   │
└─────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────┐
│  samples_to_differences()       │
│  Delta encoding: [1,3,6] →      │
│                  [1,2,3]        │
└─────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────┐
│  Compression Algorithm          │
│  (RLE/ZIP/PIZ/PXR24/B44)        │
└─────────────────────────────────┘
         │
         ▼
Compressed Bytes (to file)
```

### Decompression Post-processing

```
Compressed Bytes (from file)
         │
         ▼
┌─────────────────────────────────┐
│  Decompression Algorithm        │
│  (reverse of above)             │
└─────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────┐
│  differences_to_samples()       │
│  Undo delta: [1,2,3] →          │
│              [1,3,6]            │
└─────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────┐
│  interleave_byte_blocks()       │
│  Reorder: [ABCABC] → [AABBCC]   │
└─────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────┐
│  Convert to Native Endian       │
│  (le_to_sample for each type)   │
└─────────────────────────────────┘
         │
         ▼
Native Endian Bytes (to user)
```

## Parallel Processing

### Parallel Reading

```
File → Reader → Chunks
                  │
       ┌──────────┴──────────┐
       ▼                     ▼
  Decompress              Decompress
  (thread 1)              (thread 2)
       │                     │
       └──────────┬──────────┘
                  ▼
            mpsc::channel
                  │
                  ▼
            Assemble Image
            (main thread)
```

### Parallel Writing

```
Image Data
     │
     ▼
Generate Blocks
     │
┌────┴────┐
▼         ▼
Compress  Compress    (rayon par_iter)
│         │
└────┬────┘
     ▼
mpsc::channel
     │
     ▼
Write to File
(main thread)
```

## File Format Layout

```
OpenEXR File:
┌─────────────────────────────────────┐
│ Magic Number (4 bytes)              │  0x762f3101
├─────────────────────────────────────┤
│ Version + Flags (4 bytes)           │  Version 2 + feature bits
├─────────────────────────────────────┤
│ Header 1                            │
│   - Attributes (name=value pairs)   │
│   - 0x00 terminator                 │
├─────────────────────────────────────┤
│ [Header 2...N if multipart]         │
│   - 0x00 terminator after last      │
├─────────────────────────────────────┤
│ Offset Table 1                      │  Array of u64 chunk offsets
├─────────────────────────────────────┤
│ [Offset Table 2...N if multipart]   │
├─────────────────────────────────────┤
│ Chunk 1                             │
│   - Part number (if multipart)      │
│   - Coordinates (tile or scanline)  │
│   - Compressed pixel data           │
├─────────────────────────────────────┤
│ Chunk 2...M                         │
└─────────────────────────────────────┘
```

## See Also

- [Architecture Overview](./architecture.md) - High-level design
- [Compression Algorithms](./compression.md) - Algorithm details
