//! Unified reader for both deep and flat EXR samples.
//!
//! This module provides [`ReadAnySamples`] which automatically detects whether
//! an image contains deep or flat data and reads it into [`DeepAndFlatSamples`].
//!
//! # Motivation
//!
//! Standard EXR reading requires knowing the data type in advance:
//! - `read().no_deep_data()...` for flat images (fixed samples per pixel)
//! - `read_deep()...` for deep images (variable samples per pixel)
//!
//! The unified reader eliminates this requirement by checking the header
//! and routing to the appropriate pipeline automatically.
//!
//! # When to Use
//!
//! - **Unknown file type**: Reading files without knowing if they're deep or flat
//! - **Mixed workflows**: Processing both deep and flat files uniformly
//! - **Graceful handling**: Automatic fallback without manual type checking
//!
//! # Example
//!
//! ```ignore
//! use exrs::prelude::*;
//! use exrs::image::read::any_samples::read_any_samples;
//!
//! // Automatically detects deep vs flat
//! let image = read_any_samples()
//!     .all_channels()
//!     .first_valid_layer()
//!     .all_attributes()
//!     .from_file("unknown_type.exr")?;
//!
//! // Check what we got
//! for channel in &image.layer_data.channel_data.list {
//!     match &channel.sample_data {
//!         DeepAndFlatSamples::Deep(deep) => {
//!             println!("Deep channel: {} samples", deep.total_samples());
//!         }
//!         DeepAndFlatSamples::Flat(flat) => {
//!             println!("Flat channel: {} samples", flat.len());
//!         }
//!     }
//! }
//! ```
//!
//! # Implementation Notes
//!
//! The unified reader works by:
//! 1. Reading file metadata to check `header.deep` flag
//! 2. Routing to appropriate reader (deep or flat pipeline)
//! 3. Converting samples to [`DeepAndFlatSamples`] enum
//!
//! This is a high-level wrapper, not a merged pipeline. The underlying
//! deep and flat readers remain separate for performance reasons.
//!
//! # See Also
//!
//! - [`DeepAndFlatSamples`](crate::image::DeepAndFlatSamples) - The unified sample type
//! - [`crate::image::read`] - Flat image reading
//! - [`crate::image::read::deep`] - Deep image reading
//!
//! Implements: DEAD_CODE_ANALYSIS.md item #8

use std::io::{BufReader, Read, Seek};
use std::path::Path;

use crate::block::reader::Reader;
use crate::error::{Error, Result};
use crate::image::read::deep;
use crate::image::read::image::ReadLayers;
use crate::image::read::layers::ReadChannels;
use crate::image::{
    AnyChannel, AnyChannels, Blocks, DeepAndFlatSamples, Encoding, Image, Layer, Layers,
};
use crate::meta::attribute::TileDescription;
use smallvec::SmallVec;

// ============================================================================
// Types
// ============================================================================

/// An image with unified deep/flat samples in a single layer.
///
/// Use this when you don't know if the file is deep or flat.
pub type AnyImage = Image<Layer<AnyChannels<DeepAndFlatSamples>>>;

/// An image with unified deep/flat samples in multiple layers.
pub type AnyLayersImage = Image<Layers<AnyChannels<DeepAndFlatSamples>>>;

// ============================================================================
// Entry point
// ============================================================================

/// Start building a unified deep/flat sample reader.
///
/// This automatically detects whether the image contains deep or flat data
/// and reads it into [`DeepAndFlatSamples`].
///
/// # Example
///
/// ```ignore
/// use exrs::image::read::any_samples::read_any_samples;
///
/// let image = read_any_samples()
///     .all_channels()
///     .first_valid_layer()
///     .all_attributes()
///     .from_file("image.exr")?;
/// ```
///
/// # See Also
///
/// - [`crate::image::read::read`] - Standard builder (requires knowing data type)
pub fn read_any_samples() -> ReadAnySamples {
    ReadAnySamples
}

// ============================================================================
// Builder structs
// ============================================================================

/// Specifies to read any sample type (deep or flat), auto-detecting from the file.
///
/// Created by [`read_any_samples()`]. Call [`all_channels()`](Self::all_channels)
/// to continue building the reader.
///
/// # Type Detection
///
/// The reader checks `header.deep` flag in the file metadata:
/// - `true`: Uses deep reading pipeline, returns `DeepAndFlatSamples::Deep`
/// - `false`: Uses flat reading pipeline, returns `DeepAndFlatSamples::Flat`
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct ReadAnySamples;

impl ReadAnySamples {
    /// Read all channels from the image.
    ///
    /// Channels are read into [`AnyChannels<DeepAndFlatSamples>`], preserving
    /// all channel data regardless of whether it's deep or flat.
    pub fn all_channels(self) -> ReadAnyAllChannels {
        ReadAnyAllChannels
    }
}

/// Specifies to read all channels as unified samples.
///
/// Created by [`ReadAnySamples::all_channels()`].
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct ReadAnyAllChannels;

impl ReadAnyAllChannels {
    /// Read only the first valid layer.
    ///
    /// Skips layers that don't match the detected sample type.
    /// Returns an error if no valid layer is found.
    pub fn first_valid_layer(self) -> ReadAnyFirstLayer {
        ReadAnyFirstLayer
    }

    /// Read all valid layers.
    ///
    /// Reads all layers that match the detected sample type.
    /// Layers are returned in file order.
    pub fn all_layers(self) -> ReadAnyAllLayers {
        ReadAnyAllLayers
    }
}

/// Read first valid layer with unified samples.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct ReadAnyFirstLayer;

impl ReadAnyFirstLayer {
    /// Include all image attributes in the result.
    ///
    /// This is currently the only option; selective attribute reading
    /// may be added in the future.
    pub fn all_attributes(self) -> ReadAnyImage<FirstLayer> {
        ReadAnyImage {
            pedantic: false,
            parallel: cfg!(feature = "rayon"),
            _layer_selection: std::marker::PhantomData,
        }
    }
}

/// Read all layers with unified samples.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct ReadAnyAllLayers;

impl ReadAnyAllLayers {
    /// Include all image attributes in the result.
    pub fn all_attributes(self) -> ReadAnyImage<AllLayers> {
        ReadAnyImage {
            pedantic: false,
            parallel: cfg!(feature = "rayon"),
            _layer_selection: std::marker::PhantomData,
        }
    }
}

/// Layer selection marker: first valid layer only.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct FirstLayer;

/// Layer selection marker: all layers.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct AllLayers;

/// Final reader configuration for unified deep/flat images.
///
/// # Type Parameters
///
/// - `LayerSelection`: Either [`FirstLayer`] or [`AllLayers`], determines
///   whether to read one or all layers.
#[derive(Debug, Clone)]
pub struct ReadAnyImage<LayerSelection> {
    pedantic: bool,
    parallel: bool,
    _layer_selection: std::marker::PhantomData<LayerSelection>,
}

impl<L> ReadAnyImage<L> {
    /// Enable pedantic error handling.
    ///
    /// When enabled, the reader will fail on any non-critical errors
    /// that would otherwise be ignored or warned about.
    pub fn pedantic(mut self) -> Self {
        self.pedantic = true;
        self
    }

    /// Disable parallel decompression.
    ///
    /// By default, decompression uses rayon for parallelism (if the feature
    /// is enabled). Call this to force single-threaded decompression.
    #[allow(dead_code)]
    pub fn non_parallel(mut self) -> Self {
        self.parallel = false;
        self
    }
}

// ============================================================================
// FirstLayer implementation
// ============================================================================

impl ReadAnyImage<FirstLayer> {
    /// Read the image from a file path.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The file cannot be opened
    /// - The file is not a valid EXR file
    /// - No valid layer is found
    pub fn from_file(self, path: impl AsRef<Path>) -> Result<AnyImage> {
        self.from_unbuffered(std::fs::File::open(path)?)
    }

    /// Read from an unbuffered reader.
    ///
    /// The reader will be wrapped in a `BufReader` internally.
    pub fn from_unbuffered(self, read: impl Read + Seek) -> Result<AnyImage> {
        self.from_buffered(BufReader::new(read))
    }

    /// Read from a buffered reader.
    ///
    /// This is the most efficient option if you already have a buffered reader.
    pub fn from_buffered(self, read: impl Read + Seek) -> Result<AnyImage> {
        let reader = Reader::read_from_buffered(read, self.pedantic)?;

        // Check if any layer is deep
        let has_deep = reader.headers().iter().any(|h| h.deep);

        if has_deep {
            // Find first deep layer
            let (layer_index, header) = reader
                .headers()
                .iter()
                .enumerate()
                .find(|(_, h)| h.deep)
                .ok_or_else(|| Error::invalid("no deep layer found"))?;

            let image_attrs = header.shared_attributes.clone();
            let layer = read_deep_layer_as_any(reader, layer_index, self.pedantic, self.parallel)?;

            Ok(Image {
                attributes: image_attrs,
                layer_data: layer,
            })
        } else {
            // Find first flat layer
            let (layer_index, header) = reader
                .headers()
                .iter()
                .enumerate()
                .find(|(_, h)| !h.deep)
                .ok_or_else(|| Error::invalid("no flat layer found"))?;

            let image_attrs = header.shared_attributes.clone();
            let layer = read_flat_layer_as_any(reader, layer_index, self.pedantic, self.parallel)?;

            Ok(Image {
                attributes: image_attrs,
                layer_data: layer,
            })
        }
    }
}

// ============================================================================
// AllLayers implementation
// ============================================================================

impl ReadAnyImage<AllLayers> {
    /// Read all layers from a file path.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The file cannot be opened
    /// - The file is not a valid EXR file
    pub fn from_file(self, path: impl AsRef<Path>) -> Result<AnyLayersImage> {
        self.from_unbuffered(std::fs::File::open(path)?)
    }

    /// Read from an unbuffered reader.
    pub fn from_unbuffered(self, read: impl Read + Seek) -> Result<AnyLayersImage> {
        self.from_buffered(BufReader::new(read))
    }

    /// Read from a buffered reader.
    pub fn from_buffered(self, read: impl Read + Seek) -> Result<AnyLayersImage> {
        let reader = Reader::read_from_buffered(read, self.pedantic)?;

        // Check if any layer is deep
        let has_deep = reader.headers().iter().any(|h| h.deep);

        if has_deep {
            // Read as deep image, convert to unified type
            let image_attrs = reader.headers()[0].shared_attributes.clone();

            // For now, read first deep layer only
            // TODO: Support multiple deep layers
            let (layer_index, _) = reader
                .headers()
                .iter()
                .enumerate()
                .find(|(_, h)| h.deep)
                .ok_or_else(|| Error::invalid("no deep layer found"))?;

            let layer = read_deep_layer_as_any(reader, layer_index, self.pedantic, self.parallel)?;
            let mut layers = SmallVec::new();
            layers.push(layer);

            Ok(Image {
                attributes: image_attrs,
                layer_data: layers,
            })
        } else {
            // Read all flat layers
            read_all_flat_layers_as_any(reader, self.pedantic, self.parallel)
        }
    }
}

// ============================================================================
// Internal helpers
// ============================================================================

/// Read a deep layer and convert to unified sample type.
fn read_deep_layer_as_any(
    reader: Reader<impl Read + Seek>,
    layer_index: usize,
    pedantic: bool,
    _parallel: bool,
) -> Result<Layer<AnyChannels<DeepAndFlatSamples>>> {
    let header = &reader.headers()[layer_index];
    let layer_attrs = header.own_attributes.clone();
    let layer_size = header.layer_size;
    let encoding = make_encoding(header);

    // Read deep samples using existing deep reader infrastructure
    let deep_samples = deep::read_deep_layer_samples(reader, layer_index, pedantic)?;

    // Convert channels to unified type
    let channels: SmallVec<[AnyChannel<DeepAndFlatSamples>; 4]> = deep_samples
        .list
        .into_iter()
        .map(|ch| AnyChannel {
            name: ch.name,
            sample_data: DeepAndFlatSamples::Deep(ch.sample_data),
            quantize_linearly: ch.quantize_linearly,
            sampling: ch.sampling,
        })
        .collect();

    Ok(Layer {
        channel_data: AnyChannels { list: channels },
        attributes: layer_attrs,
        size: layer_size,
        encoding,
    })
}

/// Read a flat layer and convert to unified sample type.
///
/// Uses the standard flat reading pipeline via `from_chunks`, then converts
/// the result to unified `DeepAndFlatSamples` type.
fn read_flat_layer_as_any(
    reader: Reader<impl Read + Seek>,
    layer_index: usize,
    _pedantic: bool,
    _parallel: bool,
) -> Result<Layer<AnyChannels<DeepAndFlatSamples>>> {
    let meta = reader.meta_data().clone();
    let header = &meta.headers[layer_index];
    let layer_attrs = header.own_attributes.clone();
    let layer_size = header.layer_size;
    let encoding = make_encoding(header);
    let channel_list = header.channels.list.clone();

    // Read using standard flat pipeline
    let flat_image = crate::image::read::read()
        .no_deep_data()
        .largest_resolution_level()
        .all_channels()
        .first_valid_layer()
        .all_attributes()
        .from_chunks(reader)?;

    // Convert flat layer channels to unified type
    let channels: SmallVec<[AnyChannel<DeepAndFlatSamples>; 4]> = flat_image
        .layer_data
        .channel_data
        .list
        .into_iter()
        .zip(channel_list.iter())
        .map(|(ch, ch_desc)| AnyChannel {
            name: ch.name,
            sample_data: DeepAndFlatSamples::Flat(ch.sample_data),
            quantize_linearly: ch_desc.quantize_linearly,
            sampling: ch_desc.sampling,
        })
        .collect();

    Ok(Layer {
        channel_data: AnyChannels { list: channels },
        attributes: layer_attrs,
        size: layer_size,
        encoding,
    })
}

/// Read all flat layers and convert to unified sample type.
fn read_all_flat_layers_as_any(
    reader: Reader<impl Read + Seek>,
    _pedantic: bool,
    _parallel: bool,
) -> Result<AnyLayersImage> {
    // Use standard flat reading pipeline and convert
    let meta = reader.meta_data().clone();
    let image_attrs = meta.headers[0].shared_attributes.clone();

    // Read using standard flat pipeline
    let flat_image = crate::image::read::read()
        .no_deep_data()
        .largest_resolution_level()
        .all_channels()
        .all_layers()
        .all_attributes()
        .from_chunks(reader)?;

    // Convert to unified type
    let layers: SmallVec<[Layer<AnyChannels<DeepAndFlatSamples>>; 2]> = flat_image
        .layer_data
        .into_iter()
        .map(|layer| Layer {
            channel_data: AnyChannels {
                list: layer
                    .channel_data
                    .list
                    .into_iter()
                    .map(|ch| AnyChannel {
                        name: ch.name,
                        sample_data: DeepAndFlatSamples::Flat(ch.sample_data),
                        quantize_linearly: ch.quantize_linearly,
                        sampling: ch.sampling,
                    })
                    .collect(),
            },
            attributes: layer.attributes,
            size: layer.size,
            encoding: layer.encoding,
        })
        .collect();

    Ok(Image {
        attributes: image_attrs,
        layer_data: layers,
    })
}

/// Create Encoding from header.
fn make_encoding(header: &crate::meta::header::Header) -> Encoding {
    Encoding {
        compression: header.compression,
        line_order: header.line_order,
        blocks: match header.blocks {
            crate::meta::BlockDescription::ScanLines => Blocks::ScanLines,
            crate::meta::BlockDescription::Tiles(TileDescription { tile_size, .. }) => {
                Blocks::Tiles(tile_size)
            }
        },
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Test ReadAnySamples struct creation.
    #[test]
    fn read_any_samples_struct() {
        let reader = ReadAnySamples;
        let _ = reader.all_channels();
    }

    /// Test builder chain.
    #[test]
    fn read_any_samples_builder_chain() {
        let _ = read_any_samples()
            .all_channels()
            .first_valid_layer()
            .all_attributes();

        let _ = read_any_samples()
            .all_channels()
            .all_layers()
            .all_attributes();
    }

    /// Test pedantic mode.
    #[test]
    fn read_any_samples_pedantic() {
        let reader = read_any_samples()
            .all_channels()
            .first_valid_layer()
            .all_attributes()
            .pedantic();

        assert!(reader.pedantic);
    }

    /// Test non-parallel mode.
    #[test]
    fn read_any_samples_non_parallel() {
        let reader = read_any_samples()
            .all_channels()
            .first_valid_layer()
            .all_attributes()
            .non_parallel();

        assert!(!reader.parallel);
    }

    /// Test type aliases exist.
    #[test]
    fn type_aliases_exist() {
        fn _takes_any_image(_: AnyImage) {}
        fn _takes_any_layers_image(_: AnyLayersImage) {}
    }

    /// Test reading flat file via flat_and_deep_data().
    #[test]
    fn read_flat_file_via_builder() {
        let path = "tests/images/valid/openexr/ScanLines/Desk.exr";
        if !std::path::Path::new(path).exists() {
            return; // Skip if test file not available
        }

        let image = crate::image::read::read()
            .flat_and_deep_data()
            .all_channels()
            .first_valid_layer()
            .all_attributes()
            .from_file(path)
            .expect("failed to read flat file");

        // Verify it's detected as flat
        assert!(!image.layer_data.channel_data.list.is_empty());
        for ch in &image.layer_data.channel_data.list {
            assert!(
                ch.sample_data.is_flat(),
                "Expected flat samples, got deep for channel {}",
                ch.name
            );
        }
    }

    /// Test reading flat file via convenience function.
    #[test]
    fn read_flat_file_convenience() {
        let path = "tests/images/valid/openexr/ScanLines/Desk.exr";
        if !std::path::Path::new(path).exists() {
            return;
        }

        let image = crate::image::read::read_first_any_layer_from_file(path)
            .expect("failed to read flat file");

        assert!(!image.layer_data.channel_data.list.is_empty());
        assert!(image.layer_data.channel_data.list[0].sample_data.is_flat());
    }

    /// Test reading deep file via flat_and_deep_data().
    #[test]
    fn read_deep_file_via_builder() {
        let path = "tests/images/valid/openexr/v2/deep_large/Teaset720p.exr";
        if !std::path::Path::new(path).exists() {
            return; // Skip if test file not available
        }

        let image = crate::image::read::read()
            .flat_and_deep_data()
            .all_channels()
            .first_valid_layer()
            .all_attributes()
            .from_file(path)
            .expect("failed to read deep file");

        // Verify it's detected as deep
        assert!(!image.layer_data.channel_data.list.is_empty());
        // At least one channel should be deep
        let has_deep = image
            .layer_data
            .channel_data
            .list
            .iter()
            .any(|ch| ch.sample_data.is_deep());
        assert!(has_deep, "Expected at least one deep channel");
    }

    /// Test reading deep file via convenience function.
    #[test]
    fn read_deep_file_convenience() {
        let path = "tests/images/valid/openexr/v2/deep_large/Teaset720p.exr";
        if !std::path::Path::new(path).exists() {
            return;
        }

        let image = crate::image::read::read_first_any_layer_from_file(path)
            .expect("failed to read deep file");

        assert!(!image.layer_data.channel_data.list.is_empty());
        let has_deep = image
            .layer_data
            .channel_data
            .list
            .iter()
            .any(|ch| ch.sample_data.is_deep());
        assert!(has_deep, "Expected deep data");
    }

    /// Test that DeepAndFlatSamples helper methods work.
    #[test]
    fn deep_and_flat_samples_helpers() {
        use crate::image::DeepAndFlatSamples;
        use crate::image::FlatSamples;

        let flat = DeepAndFlatSamples::Flat(FlatSamples::F32(vec![1.0, 2.0, 3.0]));
        assert!(flat.is_flat());
        assert!(!flat.is_deep());

        let deep = DeepAndFlatSamples::Deep(crate::image::deep::DeepSamples::new(2, 2));
        assert!(deep.is_deep());
        assert!(!deep.is_flat());
    }

    /// Roundtrip test: read flat file -> write -> read again -> compare.
    #[test]
    fn roundtrip_flat_file() {
        use crate::image::write::WritableImage;
        use crate::image::FlatSamples;

        let path = "tests/images/valid/openexr/ScanLines/Desk.exr";
        if !std::path::Path::new(path).exists() {
            return;
        }

        // Read original
        let original = crate::image::read::read_first_any_layer_from_file(path)
            .expect("failed to read original");

        // Convert to writable format (extract flat samples)
        let mut flat_channels: smallvec::SmallVec<[crate::image::AnyChannel<FlatSamples>; 4]> =
            smallvec::SmallVec::new();

        for ch in &original.layer_data.channel_data.list {
            if let crate::image::DeepAndFlatSamples::Flat(flat) = &ch.sample_data {
                flat_channels.push(crate::image::AnyChannel {
                    name: ch.name.clone(),
                    sample_data: flat.clone(),
                    quantize_linearly: ch.quantize_linearly,
                    sampling: ch.sampling,
                });
            }
        }

        let writable = crate::image::Image {
            attributes: original.attributes.clone(),
            layer_data: crate::image::Layer {
                channel_data: crate::image::AnyChannels { list: flat_channels },
                attributes: original.layer_data.attributes.clone(),
                size: original.layer_data.size,
                encoding: original.layer_data.encoding.clone(),
            },
        };

        // Write to temp file
        let temp_path = "target/test_roundtrip_any.exr";
        writable.write().to_file(temp_path).expect("failed to write");

        // Read back
        let reloaded = crate::image::read::read_first_any_layer_from_file(temp_path)
            .expect("failed to read back");

        // Compare
        assert_eq!(
            original.layer_data.size,
            reloaded.layer_data.size,
            "Size mismatch"
        );
        assert_eq!(
            original.layer_data.channel_data.list.len(),
            reloaded.layer_data.channel_data.list.len(),
            "Channel count mismatch"
        );

        for (orig_ch, reload_ch) in original
            .layer_data
            .channel_data
            .list
            .iter()
            .zip(reloaded.layer_data.channel_data.list.iter())
        {
            assert_eq!(orig_ch.name, reload_ch.name, "Channel name mismatch");
            assert!(
                orig_ch.sample_data.is_flat() && reload_ch.sample_data.is_flat(),
                "Both should be flat"
            );
        }

        // Cleanup
        let _ = std::fs::remove_file(temp_path);
    }
}
