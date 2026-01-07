# Block Module

The `exr::block` module provides low-level access to EXR pixel blocks.

## Overview

Use this module when you need:
- Custom compression handling
- Streaming processing
- Direct block manipulation
- Memory-constrained environments

## BlockIndex

Identifies a block within an image:

```rust
pub struct BlockIndex {
    /// Layer index (for multipart)
    pub layer: usize,
    
    /// Mip/rip level
    pub level: Vec2<usize>,
    
    /// Block position in pixels
    pub pixel_position: Vec2<usize>,
    
    /// Block size in pixels
    pub pixel_size: Vec2<usize>,
}
```

## UncompressedBlock

Decompressed pixel data:

```rust
pub struct UncompressedBlock {
    /// Block identification
    pub index: BlockIndex,
    
    /// Raw pixel bytes (native endian)
    pub data: Vec<u8>,
}

impl UncompressedBlock {
    /// Decompress from chunk
    pub fn decompress_chunk(
        chunk: Chunk,
        meta_data: &MetaData,
        pedantic: bool,
    ) -> Result<Self>;
    
    /// Compress to chunk
    pub fn compress_to_chunk(
        &self,
        headers: &Headers,
    ) -> Result<Chunk>;
    
    /// Iterate lines
    pub fn lines(&self, channels: &ChannelList) -> impl Iterator<Item = LineRef>;
    
    /// Mutable line access
    pub fn lines_mut(&mut self, channels: &ChannelList) -> impl Iterator<Item = LineRefMut>;
}
```

## Chunk

Compressed block data:

```rust
pub struct Chunk {
    /// Layer this chunk belongs to
    pub layer_index: usize,
    
    /// Compressed data
    pub compressed_block: CompressedBlock,
}

pub enum CompressedBlock {
    /// Flat scanline block
    ScanLine {
        y_coordinate: i32,
        compressed_pixels: Vec<u8>,
    },
    
    /// Flat tile block
    Tile {
        tile_position: Vec2<usize>,
        level: Vec2<usize>,
        compressed_pixels: Vec<u8>,
    },
    
    /// Deep scanline block
    DeepScanLine {
        y_coordinate: i32,
        compressed_sample_offset_table: Vec<u8>,
        compressed_samples: Vec<u8>,
        decompressed_samples_byte_count: usize,
    },
    
    /// Deep tile block
    DeepTile {
        tile_position: Vec2<usize>,
        level: Vec2<usize>,
        compressed_sample_offset_table: Vec<u8>,
        compressed_samples: Vec<u8>,
        decompressed_samples_byte_count: usize,
    },
}
```

## Reading

### Reader

Low-level file reader:

```rust
pub struct Reader<R> {
    meta_data: MetaData,
    // ...
}

impl<R: Read + Seek> Reader<R> {
    /// Read from file
    pub fn read_from_file(
        path: impl AsRef<Path>,
        pedantic: bool,
    ) -> Result<Self>;
    
    /// Read from buffered reader
    pub fn read_from_buffered(
        read: R,
        pedantic: bool,
    ) -> Result<Self>;
    
    /// Get metadata
    pub fn meta_data(&self) -> &MetaData;
    
    /// Get all chunks iterator
    pub fn all_chunks(self, pedantic: bool) -> ChunksReader<R>;
    
    /// Decompress all blocks in parallel
    pub fn decompress_parallel(
        self,
        pedantic: bool,
    ) -> ParallelBlockDecompressor<R>;
}
```

### ChunksReader

Iterator over chunks:

```rust
pub struct ChunksReader<R> { /* ... */ }

impl<R: Read> Iterator for ChunksReader<R> {
    type Item = Result<Chunk>;
}
```

### ParallelBlockDecompressor

Parallel decompression:

```rust
pub struct ParallelBlockDecompressor<R> { /* ... */ }

impl<R: Read + Send> Iterator for ParallelBlockDecompressor<R> {
    type Item = Result<UncompressedBlock>;
}
```

## Writing

### block::write()

Entry point for writing:

```rust
pub fn write<W: Write + Seek>(
    write: W,
    headers: Headers,
    pedantic: bool,
    write_chunks: impl FnOnce(MetaData, &mut ChunksWriter<W>) -> UnitResult,
) -> UnitResult;
```

### ChunksWriter

Write chunks to file:

```rust
pub struct ChunksWriter<W> { /* ... */ }

impl<W: Write + Seek> ChunksWriter<W> {
    /// Write a single chunk
    pub fn write_chunk(&mut self, chunk: Chunk) -> UnitResult;
    
    /// Compress and write all blocks sequentially
    pub fn compress_all_blocks_sequential<B>(
        &mut self,
        headers: &Headers,
        blocks: B,
    ) -> UnitResult
    where
        B: Iterator<Item = UncompressedBlock>;
    
    /// Compress and write all blocks in parallel
    #[cfg(feature = "rayon")]
    pub fn compress_all_blocks_parallel<B>(
        &mut self,
        headers: &Headers,
        blocks: B,
    ) -> UnitResult
    where
        B: Iterator<Item = UncompressedBlock> + Send;
}
```

## Lines

### LineIndex

Line identification:

```rust
pub struct LineIndex {
    /// Channel index
    pub channel: usize,
    
    /// Y position within block
    pub position: Vec2<usize>,
}
```

### LineRef / LineRefMut

Line access:

```rust
pub struct LineRef<'a> {
    pub index: LineIndex,
    pub data: &'a [u8],
}

pub struct LineRefMut<'a> {
    pub index: LineIndex,
    pub data: &'a mut [u8],
}
```

## Sample Types

### Sample

Dynamic sample value:

```rust
pub enum Sample {
    F16(f16),
    F32(f32),
    U32(u32),
}

impl Sample {
    /// Convert to f16
    pub fn to_f16(self) -> f16;
    
    /// Convert to f32
    pub fn to_f32(self) -> f32;
    
    /// Convert to u32
    pub fn to_u32(self) -> u32;
    
    /// Check if NaN
    pub fn is_nan(self) -> bool;
    
    /// Check if infinite
    pub fn is_infinite(self) -> bool;
}
```

## Deep Blocks

### DeepUncompressedBlock

Decompressed deep data:

```rust
pub struct DeepUncompressedBlock {
    pub index: BlockIndex,
    pub samples: DeepSamples,
}
```

### Deep Decompression

```rust
pub fn decompress_deep_scanline_block(
    chunk: &Chunk,
    header: &Header,
    pedantic: bool,
) -> Result<DeepUncompressedBlock>;

pub fn decompress_deep_tile_block(
    chunk: &Chunk,
    header: &Header,
    pedantic: bool,
) -> Result<DeepUncompressedBlock>;
```

## Example: Custom Reading

```rust
use exr::block::{self, Reader, UncompressedBlock};

fn custom_read(path: &str) -> Result<(), exr::error::Error> {
    let reader = Reader::read_from_file(path, false)?;
    let meta = reader.meta_data().clone();
    
    for chunk_result in reader.all_chunks(false) {
        let chunk = chunk_result?;
        let block = UncompressedBlock::decompress_chunk(chunk, &meta, false)?;
        
        println!("Block at {:?}, size {:?}", 
            block.index.pixel_position,
            block.index.pixel_size);
        
        // Access lines
        for line in block.lines(&meta.headers[block.index.layer].channels) {
            println!("  Channel {}, y={}, {} bytes",
                line.index.channel,
                line.index.position.y(),
                line.data.len());
        }
    }
    
    Ok(())
}
```

## Example: Custom Writing

```rust
use exr::block::{self, UncompressedBlock, BlockIndex};
use exr::meta::{Header, MetaData};

fn custom_write(path: &str, headers: Vec<Header>) -> UnitResult {
    let file = std::fs::File::create(path)?;
    
    block::write(
        file,
        headers.into(),
        true,  // pedantic
        |meta, writer| {
            // Generate blocks
            for layer_index in 0..meta.headers.len() {
                let header = &meta.headers[layer_index];
                
                for y in (0..header.data_size.height())
                    .step_by(header.compression.scan_lines_per_block())
                {
                    let block = generate_block(layer_index, y, header);
                    let chunk = block.compress_to_chunk(&meta.headers)?;
                    writer.write_chunk(chunk)?;
                }
            }
            Ok(())
        }
    )
}
```

## See Also

- [Compression Module](./compression.md) - Compression algorithms
- [Deep Data API](./deep.md) - Deep block handling
- [Developer Guide: Data Flow](../developer-guide/dataflow.md)
