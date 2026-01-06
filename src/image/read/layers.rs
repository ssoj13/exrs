//! How to read either a single or a list of layers.
//!
//! This module provides three strategies for reading layers from OpenEXR images:
//!
//! - [`ReadAllLayers`]: Read all layers, fail if any layer is invalid.
//! - [`ReadFirstValidLayer`]: Read only the first layer that matches requirements.
//! - [`ReadAllValidLayers`]: Read all valid layers, silently skipping invalid ones.
//!
//! # Example: Reading All Valid Layers
//!
//! ```ignore
//! use exrs::prelude::*;
//!
//! // Read all layers with RGB channels, skip layers without them
//! let image = read()
//!     .no_deep_data()
//!     .largest_resolution_level()
//!     .all_channels()
//!     .all_valid_layers()  // Won't fail if some layers are invalid
//!     .all_attributes()
//!     .from_file("multi_layer.exr")?;
//!
//! println!("Successfully read {} layers", image.layer_data.len());
//! ```

use crate::block::chunk::TileCoordinates;
use crate::block::{BlockIndex, UncompressedBlock};
use crate::error::{Error, Result, UnitResult};
use crate::image::read::image::{LayersReader, ReadLayers};
use crate::image::*;
use crate::math::Vec2;
use crate::meta::header::{Header, LayerAttributes};
use crate::meta::MetaData;

/// Specify to read all channels, aborting if any one is invalid.
/// [`ReadRgbaChannels`] or [`ReadAnyChannels<ReadFlatSamples>`].
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ReadAllLayers<ReadChannels> {
    /// The channel reading specification
    pub read_channels: ReadChannels,
}

/// Specify to read only the first layer which meets the previously specified requirements.
/// Note: For deep data, use `read().deep_data()` instead of `read().no_deep_data()`.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ReadFirstValidLayer<ReadChannels> {
    /// The channel reading specification
    pub read_channels: ReadChannels,
}

/// Specify to read all layers that match the requirements, silently skipping invalid ones.
///
/// Unlike [`ReadAllLayers`] which fails if any layer is invalid, this strategy
/// uses `flat_map` to filter out layers that don't match the channel requirements.
/// This is useful for robust reading of files with mixed layer types.
///
/// # When to Use
///
/// - Files may contain layers with different channel configurations
/// - You want to read what you can and ignore incompatible layers
/// - Graceful degradation is preferred over strict validation
///
/// # Result
///
/// Returns `Layers<C>` (a `SmallVec`) which may be empty if no layers matched.
/// Check `layer_data.is_empty()` if you need at least one layer.
///
/// # Example
///
/// ```ignore
/// use exrs::prelude::*;
///
/// let image = read()
///     .no_deep_data()
///     .largest_resolution_level()
///     .rgb_channels(create_pixels, set_pixel)
///     .all_valid_layers()  // Skips layers without RGB
///     .all_attributes()
///     .from_file("mixed_layers.exr")?;
///
/// if image.layer_data.is_empty() {
///     println!("No RGB layers found");
/// } else {
///     println!("Found {} RGB layers", image.layer_data.len());
/// }
/// ```
///
/// # See Also
///
/// - [`ReadAllLayers`]: Stricter reading that fails on any invalid layer.
/// - [`ReadFirstValidLayer`]: Read only the first matching layer.
///
/// Implements: DEAD_CODE_ANALYSIS.md item #10
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ReadAllValidLayers<ReadChannels> {
    /// The channel reading specification
    pub read_channels: ReadChannels,
}

/// A template that creates a [`ChannelsReader`] once for all channels per layer.
pub trait ReadChannels<'s> {
    /// The type of the temporary channels reader
    type Reader: ChannelsReader;

    /// Create a single reader for all channels of a specific layer
    fn create_channels_reader(&'s self, header: &Header) -> Result<Self::Reader>;

    /// Read only the first layer which meets the previously specified requirements.
    /// For example, skips layers with deep data, if specified earlier.
    /// Aborts if the image contains no layers.
    /// Note: Use `read().deep_data()` pipeline for reading deep data layers.
    fn first_valid_layer(self) -> ReadFirstValidLayer<Self>
    where
        Self: Sized,
    {
        ReadFirstValidLayer {
            read_channels: self,
        }
    }

    /// Reads all layers, including an empty list. Aborts if any of the layers are invalid,
    /// even if only one of the layers contains unexpected data.
    fn all_layers(self) -> ReadAllLayers<Self>
    where
        Self: Sized,
    {
        ReadAllLayers {
            read_channels: self,
        }
    }

    /// Read all layers that match the channel requirements, silently skipping invalid ones.
    ///
    /// Unlike [`all_layers`](Self::all_layers) which fails if any layer is invalid,
    /// this method uses `flat_map` to filter out incompatible layers.
    ///
    /// # Returns
    ///
    /// An image with `Layers<C>` which may be empty if no layers matched.
    /// Use `.layer_data.is_empty()` to check if any layers were found.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use exrs::prelude::*;
    ///
    /// // Read all RGB layers, skip any non-RGB layers
    /// let image = read()
    ///     .no_deep_data()
    ///     .largest_resolution_level()
    ///     .rgb_channels(create, set)
    ///     .all_valid_layers()
    ///     .all_attributes()
    ///     .from_file("multi.exr")?;
    /// ```
    ///
    /// # See Also
    ///
    /// - [`all_layers`](Self::all_layers): Stricter, fails on any invalid layer.
    /// - [`first_valid_layer`](Self::first_valid_layer): Returns only one layer.
    ///
    /// Implements: DEAD_CODE_ANALYSIS.md item #10
    fn all_valid_layers(self) -> ReadAllValidLayers<Self>
    where
        Self: Sized,
    {
        ReadAllValidLayers {
            read_channels: self,
        }
    }
}

/// Processes pixel blocks from a file and accumulates them into a list of layers.
/// For example, `ChannelsReader` can be
/// [`SpecificChannelsReader`] or [`AnyChannelsReader<FlatSamplesReader>`].
#[derive(Debug, Clone, PartialEq)]
pub struct AllLayersReader<ChannelsReader> {
    layer_readers: SmallVec<[LayerReader<ChannelsReader>; 2]>, // TODO unpack struct?
}

/// Processes pixel blocks from a file, accumulating only valid layers.
///
/// Similar to [`AllLayersReader`] but tracks which layers were successfully
/// created, skipping invalid ones during both creation and block reading.
///
/// # Type Parameters
///
/// - `ChannelsReader`: The channels reader type (e.g., `SpecificChannelsReader`).
#[derive(Debug, Clone, PartialEq)]
pub struct AllValidLayersReader<ChannelsReader> {
    /// Layer readers for valid layers only.
    layer_readers: SmallVec<[LayerReader<ChannelsReader>; 2]>,

    /// Maps valid layer reader index to original header index.
    /// Used to filter blocks and map them to the correct reader.
    valid_layer_indices: SmallVec<[usize; 4]>,
}

/// Processes pixel blocks from a file and accumulates them into a single layers, using only the first.
/// For example, `ChannelsReader` can be
/// `SpecificChannelsReader` or `AnyChannelsReader<FlatSamplesReader>`.
#[derive(Debug, Clone, PartialEq)]
pub struct FirstValidLayerReader<ChannelsReader> {
    layer_reader: LayerReader<ChannelsReader>,
    layer_index: usize,
}

/// Processes pixel blocks from a file and accumulates them into a single layers.
/// For example, `ChannelsReader` can be
/// `SpecificChannelsReader` or `AnyChannelsReader<FlatSamplesReader>`.
#[derive(Debug, Clone, PartialEq)]
pub struct LayerReader<ChannelsReader> {
    channels_reader: ChannelsReader,
    attributes: LayerAttributes,
    size: Vec2<usize>,
    encoding: Encoding,
}

/// Processes pixel blocks from a file and accumulates them into multiple channels per layer.
pub trait ChannelsReader {
    /// The type of the resulting channel collection
    type Channels;

    /// Specify whether a single block of pixels should be loaded from the file
    fn filter_block(&self, tile: TileCoordinates) -> bool;

    /// Load a single pixel block, which has not been filtered, into the reader, accumulating the channel data
    fn read_block(&mut self, header: &Header, block: UncompressedBlock) -> UnitResult;

    /// Deliver the final accumulated channel collection for the image
    fn into_channels(self) -> Self::Channels;
}

impl<C> LayerReader<C> {
    fn new(header: &Header, channels_reader: C) -> Result<Self> {
        Ok(LayerReader {
            channels_reader,
            attributes: header.own_attributes.clone(),
            size: header.layer_size,
            encoding: Encoding {
                compression: header.compression,
                line_order: header.line_order,
                blocks: match header.blocks {
                    crate::meta::BlockDescription::ScanLines => Blocks::ScanLines,
                    crate::meta::BlockDescription::Tiles(TileDescription { tile_size, .. }) => {
                        Blocks::Tiles(tile_size)
                    }
                },
            },
        })
    }
}

impl<'s, C> ReadLayers<'s> for ReadAllLayers<C>
where
    C: ReadChannels<'s>,
{
    type Layers = Layers<<C::Reader as ChannelsReader>::Channels>;
    type Reader = AllLayersReader<C::Reader>;

    fn create_layers_reader(&'s self, headers: &[Header]) -> Result<Self::Reader> {
        let readers: Result<_> = headers
            .iter()
            .map(|header| {
                LayerReader::new(header, self.read_channels.create_channels_reader(header)?)
            })
            .collect();

        Ok(AllLayersReader {
            layer_readers: readers?,
        })
    }
}

impl<C> LayersReader for AllLayersReader<C>
where
    C: ChannelsReader,
{
    type Layers = Layers<C::Channels>;

    fn filter_block(&self, _: &MetaData, tile: TileCoordinates, block: BlockIndex) -> bool {
        let layer = self
            .layer_readers
            .get(block.layer)
            .expect("invalid layer index argument");
        layer.channels_reader.filter_block(tile)
    }

    fn read_block(&mut self, headers: &[Header], block: UncompressedBlock) -> UnitResult {
        self.layer_readers
            .get_mut(block.index.layer)
            .expect("invalid layer index argument")
            .channels_reader
            .read_block(
                headers
                    .get(block.index.layer)
                    .expect("invalid header index in block"),
                block,
            )
    }

    fn into_layers(self) -> Self::Layers {
        self.layer_readers
            .into_iter()
            .map(|layer| Layer {
                channel_data: layer.channels_reader.into_channels(),
                attributes: layer.attributes,
                size: layer.size,
                encoding: layer.encoding,
            })
            .collect()
    }
}

impl<'s, C> ReadLayers<'s> for ReadFirstValidLayer<C>
where
    C: ReadChannels<'s>,
{
    type Layers = Layer<<C::Reader as ChannelsReader>::Channels>;
    type Reader = FirstValidLayerReader<C::Reader>;

    fn create_layers_reader(&'s self, headers: &[Header]) -> Result<Self::Reader> {
        headers
            .iter()
            .enumerate()
            .flat_map(|(index, header)| {
                self.read_channels
                    .create_channels_reader(header)
                    .and_then(|reader| {
                        Ok(FirstValidLayerReader {
                            layer_reader: LayerReader::new(header, reader)?,
                            layer_index: index,
                        })
                    })
                    .ok()
            })
            .next()
            .ok_or(Error::invalid(
                "no layer in the image matched your specified requirements",
            ))
    }
}

impl<C> LayersReader for FirstValidLayerReader<C>
where
    C: ChannelsReader,
{
    type Layers = Layer<C::Channels>;

    fn filter_block(&self, _: &MetaData, tile: TileCoordinates, block: BlockIndex) -> bool {
        block.layer == self.layer_index && self.layer_reader.channels_reader.filter_block(tile)
    }

    fn read_block(&mut self, headers: &[Header], block: UncompressedBlock) -> UnitResult {
        debug_assert_eq!(
            block.index.layer, self.layer_index,
            "block should have been filtered out"
        );
        self.layer_reader
            .channels_reader
            .read_block(&headers[self.layer_index], block)
    }

    fn into_layers(self) -> Self::Layers {
        Layer {
            channel_data: self.layer_reader.channels_reader.into_channels(),
            attributes: self.layer_reader.attributes,
            size: self.layer_reader.size,
            encoding: self.layer_reader.encoding,
        }
    }
}

impl<'s, C> ReadLayers<'s> for ReadAllValidLayers<C>
where
    C: ReadChannels<'s>,
{
    type Layers = Layers<<C::Reader as ChannelsReader>::Channels>;
    type Reader = AllValidLayersReader<C::Reader>;

    fn create_layers_reader(&'s self, headers: &[Header]) -> Result<Self::Reader> {
        // Use flat_map to collect only successfully created readers
        let mut layer_readers = SmallVec::new();
        let mut valid_layer_indices = SmallVec::new();

        for (index, header) in headers.iter().enumerate() {
            // Try to create a reader; skip if it fails
            if let Ok(channels_reader) = self.read_channels.create_channels_reader(header) {
                if let Ok(layer_reader) = LayerReader::new(header, channels_reader) {
                    layer_readers.push(layer_reader);
                    valid_layer_indices.push(index);
                }
            }
        }

        Ok(AllValidLayersReader {
            layer_readers,
            valid_layer_indices,
        })
    }
}

impl<C> LayersReader for AllValidLayersReader<C>
where
    C: ChannelsReader,
{
    type Layers = Layers<C::Channels>;

    fn filter_block(&self, _: &MetaData, tile: TileCoordinates, block: BlockIndex) -> bool {
        // Check if this block's layer is in our valid layers list
        if let Some(reader_idx) = self
            .valid_layer_indices
            .iter()
            .position(|&idx| idx == block.layer)
        {
            self.layer_readers[reader_idx]
                .channels_reader
                .filter_block(tile)
        } else {
            // Layer not in valid list, skip this block
            false
        }
    }

    fn read_block(&mut self, headers: &[Header], block: UncompressedBlock) -> UnitResult {
        // Find the reader index for this layer
        let reader_idx = self
            .valid_layer_indices
            .iter()
            .position(|&idx| idx == block.index.layer)
            .expect("block should have been filtered out by filter_block");

        self.layer_readers[reader_idx].channels_reader.read_block(
            &headers[block.index.layer],
            block,
        )
    }

    fn into_layers(self) -> Self::Layers {
        self.layer_readers
            .into_iter()
            .map(|layer| Layer {
                channel_data: layer.channels_reader.into_channels(),
                attributes: layer.attributes,
                size: layer.size,
                encoding: layer.encoding,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test ReadAllValidLayers struct creation.
    #[test]
    fn read_all_valid_layers_struct() {
        #[derive(Debug, Clone, Eq, PartialEq)]
        struct DummyChannels;

        let reader = ReadAllValidLayers {
            read_channels: DummyChannels,
        };

        assert_eq!(reader.read_channels, DummyChannels);
    }

    /// Test ReadAllValidLayers debug formatting.
    #[test]
    fn read_all_valid_layers_debug() {
        #[derive(Debug, Clone, Eq, PartialEq)]
        struct DummyChannels;

        let reader = ReadAllValidLayers {
            read_channels: DummyChannels,
        };

        let debug = format!("{:?}", reader);
        assert!(debug.contains("ReadAllValidLayers"));
        assert!(debug.contains("DummyChannels"));
    }

    /// Test valid layer index mapping.
    #[test]
    fn valid_layer_indices_mapping() {
        // Simulate scenario: 5 headers, only indices 1 and 3 are valid
        let valid_indices: SmallVec<[usize; 4]> = smallvec![1, 3];

        // Block from layer 1 should map to reader index 0
        assert_eq!(valid_indices.iter().position(|&idx| idx == 1), Some(0));

        // Block from layer 3 should map to reader index 1
        assert_eq!(valid_indices.iter().position(|&idx| idx == 3), Some(1));

        // Block from layer 2 (invalid) should return None
        assert_eq!(valid_indices.iter().position(|&idx| idx == 2), None);
    }

    /// Test empty valid layers scenario.
    #[test]
    fn empty_valid_layers() {
        let valid_indices: SmallVec<[usize; 4]> = SmallVec::new();

        // No layers should match
        assert!(valid_indices.is_empty());
        assert_eq!(valid_indices.iter().position(|&idx| idx == 0), None);
    }

    /// Test all layers valid scenario.
    #[test]
    fn all_layers_valid() {
        let valid_indices: SmallVec<[usize; 4]> = smallvec![0, 1, 2];

        // All layers should be present
        assert_eq!(valid_indices.len(), 3);
        assert_eq!(valid_indices.iter().position(|&idx| idx == 0), Some(0));
        assert_eq!(valid_indices.iter().position(|&idx| idx == 1), Some(1));
        assert_eq!(valid_indices.iter().position(|&idx| idx == 2), Some(2));
    }
}
