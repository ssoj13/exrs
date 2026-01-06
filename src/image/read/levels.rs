//! How to read a set of resolution levels.
//!
//! This module provides three strategies for reading resolution levels from OpenEXR images:
//!
//! - [`ReadLargestLevel`]: Read only the highest resolution (level 0). Most common use case.
//! - [`ReadAllLevels`]: Read all mipmap/ripmap levels into a [`Levels`] structure.
//! - [`ReadSpecificLevel`]: Select a specific level using a closure. Useful for LOD systems.
//!
//! # Level Selection Example
//!
//! ```ignore
//! use exrs::prelude::*;
//!
//! // Read the level closest to 256x256 resolution
//! let image = read()
//!     .no_deep_data()
//!     .specific_resolution_level(|levels| {
//!         levels.iter()
//!             .min_by_key(|info| {
//!                 let diff_x = (info.resolution.x() as i64 - 256).abs();
//!                 let diff_y = (info.resolution.y() as i64 - 256).abs();
//!                 diff_x + diff_y
//!             })
//!             .map(|info| info.index)
//!             .unwrap_or(Vec2(0, 0))
//!     })
//!     .all_channels()
//!     .all_layers()
//!     .from_file("texture.exr")?;
//! ```

use crate::block::chunk::TileCoordinates;
use crate::block::lines::LineRef;
use crate::block::samples::*;
use crate::error::*;
use crate::image::read::any_channels::*;
use crate::image::read::specific_channels::*;
use crate::image::recursive::*;
use crate::image::*;
use crate::math::Vec2;
use crate::meta::attribute::*;
use crate::meta::header::Header;
use crate::meta::*;
use smallvec::SmallVec;

// Note: In the resulting image, the `FlatSamples` are placed
// directly inside the channels, without `LargestLevel<>` indirection
/// Specify to read only the highest resolution level, skipping all smaller variations.
/// The sample storage can be [`ReadFlatSamples`].
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ReadLargestLevel<DeepOrFlatSamples> {
    /// The sample reading specification
    pub read_samples: DeepOrFlatSamples,
}

// FIXME rgba levels???

// Read the largest level, directly, without intermediate structs
impl<DeepOrFlatSamples> ReadLargestLevel<DeepOrFlatSamples> {
    /// Read all arbitrary channels in each layer.
    pub fn all_channels(self) -> ReadAnyChannels<DeepOrFlatSamples> {
        ReadAnyChannels {
            read_samples: self.read_samples,
        }
    } // Instead of Self, the `FlatSamples` are used directly

    /// Read only layers that contain rgba channels. Skips any other channels in the layer.
    /// The alpha channel will contain the value `1.0` if no alpha channel can be found in the image.
    ///
    /// Using two closures, define how to store the pixels.
    /// The first closure creates an image, and the second closure inserts a single pixel.
    /// The type of the pixel can be defined by the second closure;
    /// it must be a tuple containing four values, each being either `f16`, `f32`, `u32` or `Sample`.
    ///
    /// Throws an error for images with deep data or subsampling.
    /// Use `specific_channels` or `all_channels` if you want to read something other than rgba.
    pub fn rgba_channels<R, G, B, A, Create, Set, Pixels>(
        self,
        create_pixels: Create,
        set_pixel: Set,
    ) -> CollectPixels<
        ReadOptionalChannel<
            ReadRequiredChannel<ReadRequiredChannel<ReadRequiredChannel<NoneMore, R>, G>, B>,
            A,
        >,
        (R, G, B, A),
        Pixels,
        Create,
        Set,
    >
    where
        R: FromNativeSample,
        G: FromNativeSample,
        B: FromNativeSample,
        A: FromNativeSample,
        Create: Fn(Vec2<usize>, &RgbaChannels) -> Pixels,
        Set: Fn(&mut Pixels, Vec2<usize>, (R, G, B, A)),
    {
        self.specific_channels()
            .required("R")
            .required("G")
            .required("B")
            .optional("A", A::from_f32(1.0))
            .collect_pixels(create_pixels, set_pixel)
    }

    /// Read only layers that contain rgb channels. Skips any other channels in the layer.
    ///
    /// Using two closures, define how to store the pixels.
    /// The first closure creates an image, and the second closure inserts a single pixel.
    /// The type of the pixel can be defined by the second closure;
    /// it must be a tuple containing three values, each being either `f16`, `f32`, `u32` or `Sample`.
    ///
    /// Throws an error for images with deep data or subsampling.
    /// Use `specific_channels` or `all_channels` if you want to read something other than rgb.
    pub fn rgb_channels<R, G, B, Create, Set, Pixels>(
        self,
        create_pixels: Create,
        set_pixel: Set,
    ) -> CollectPixels<
        ReadRequiredChannel<ReadRequiredChannel<ReadRequiredChannel<NoneMore, R>, G>, B>,
        (R, G, B),
        Pixels,
        Create,
        Set,
    >
    where
        R: FromNativeSample,
        G: FromNativeSample,
        B: FromNativeSample,
        Create: Fn(Vec2<usize>, &RgbChannels) -> Pixels,
        Set: Fn(&mut Pixels, Vec2<usize>, (R, G, B)),
    {
        self.specific_channels()
            .required("R")
            .required("G")
            .required("B")
            .collect_pixels(create_pixels, set_pixel)
    }

    /// Read only layers that contain the specified channels, skipping any other channels in the layer.
    /// Further specify which channels should be included by calling `.required("ChannelName")`
    /// or `.optional("ChannelName", default_value)` on the result of this function.
    /// Call `collect_pixels` afterwards to define the pixel container for your set of channels.
    ///
    /// Throws an error for images with deep data or subsampling.
    pub fn specific_channels(self) -> ReadZeroChannels {
        ReadZeroChannels {}
    }
}

/// Specify to read all contained resolution levels from the image, if any.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ReadAllLevels<DeepOrFlatSamples> {
    /// The sample reading specification
    pub read_samples: DeepOrFlatSamples,
}

impl<ReadDeepOrFlatSamples> ReadAllLevels<ReadDeepOrFlatSamples> {
    /// Read all arbitrary channels in each layer.
    pub fn all_channels(self) -> ReadAnyChannels<Self> {
        ReadAnyChannels { read_samples: self }
    }

    // TODO specific channels for multiple resolution levels
}

/// Information about a single resolution level.
///
/// Provided to the level selector closure in [`ReadSpecificLevel`] to help
/// users choose which resolution level to read from a mipmap or ripmap.
///
/// # Fields
///
/// - `index`: The level coordinates. For mipmaps, both components are equal (e.g., `Vec2(2, 2)`).
///   For ripmaps, components can differ (e.g., `Vec2(1, 3)` means X scaled by 2^1, Y by 2^3).
/// - `resolution`: The actual pixel dimensions at this level.
///
/// # Example
///
/// ```ignore
/// // Select the smallest level that's at least 128 pixels wide
/// |levels: &[LevelInfo]| {
///     levels.iter()
///         .filter(|info| info.resolution.x() >= 128)
///         .max_by_key(|info| info.index.x() + info.index.y())
///         .map(|info| info.index)
///         .unwrap_or(Vec2(0, 0))
/// }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LevelInfo {
    /// The level index. For mipmaps: `Vec2(n, n)`. For ripmaps: `Vec2(x_level, y_level)`.
    /// Level `Vec2(0, 0)` is always the full resolution.
    pub index: Vec2<usize>,

    /// The pixel resolution at this level. Decreases as level index increases.
    pub resolution: Vec2<usize>,
}

/// Specify to read a single resolution level selected by a user-provided closure.
///
/// This is useful for LOD (Level of Detail) systems where you want to read a specific
/// mipmap level based on viewing distance, texture budget, or other criteria.
///
/// The selector closure receives a slice of [`LevelInfo`] describing all available levels
/// and must return the `Vec2<usize>` index of the desired level.
///
/// # Type Parameters
///
/// - `DeepOrFlatSamples`: The sample reading specification (usually [`ReadFlatSamples`]).
/// - `F`: The level selector closure type.
///
/// # Example
///
/// ```ignore
/// use exrs::prelude::*;
///
/// // Read mipmap level 2 (quarter resolution)
/// let image = read()
///     .no_deep_data()
///     .specific_resolution_level(|_levels| Vec2(2, 2))
///     .all_channels()
///     .first_valid_layer()
///     .from_file("mipmapped.exr")?;
/// ```
///
/// # See Also
///
/// - [`ReadLargestLevel`]: Always reads level 0 (highest resolution).
/// - [`ReadAllLevels`]: Reads all levels into a [`Levels`] structure.
/// - [`LevelInfo`]: Information provided to the selector closure.
#[derive(Clone)]
pub struct ReadSpecificLevel<DeepOrFlatSamples, F> {
    /// The sample reading specification.
    pub read_samples: DeepOrFlatSamples,

    /// Closure that selects which level to read.
    /// Receives available levels info, returns the index of the desired level.
    pub select_level: F,
}

impl<S: std::fmt::Debug, F> std::fmt::Debug for ReadSpecificLevel<S, F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReadSpecificLevel")
            .field("read_samples", &self.read_samples)
            .field("select_level", &"<closure>")
            .finish()
    }
}

impl<DeepOrFlatSamples, F> ReadSpecificLevel<DeepOrFlatSamples, F>
where
    F: Fn(&[LevelInfo]) -> Vec2<usize>,
{
    /// Read all arbitrary channels in each layer.
    ///
    /// The selected resolution level's samples are stored directly in the channel,
    /// without the [`Levels`] wrapper used by [`ReadAllLevels`].
    pub fn all_channels(self) -> ReadAnyChannels<Self> {
        ReadAnyChannels { read_samples: self }
    }

    /// Read only layers that contain rgba channels. Skips any other channels in the layer.
    /// The alpha channel will contain the value `1.0` if no alpha channel can be found in the image.
    ///
    /// Using two closures, define how to store the pixels.
    /// The first closure creates an image, and the second closure inserts a single pixel.
    ///
    /// Throws an error for images with deep data or subsampling.
    pub fn rgba_channels<R, G, B, A, Create, Set, Pixels>(
        self,
        create_pixels: Create,
        set_pixel: Set,
    ) -> CollectPixels<
        ReadOptionalChannel<
            ReadRequiredChannel<ReadRequiredChannel<ReadRequiredChannel<NoneMore, R>, G>, B>,
            A,
        >,
        (R, G, B, A),
        Pixels,
        Create,
        Set,
    >
    where
        R: FromNativeSample,
        G: FromNativeSample,
        B: FromNativeSample,
        A: FromNativeSample,
        Create: Fn(Vec2<usize>, &RgbaChannels) -> Pixels,
        Set: Fn(&mut Pixels, Vec2<usize>, (R, G, B, A)),
    {
        self.specific_channels()
            .required("R")
            .required("G")
            .required("B")
            .optional("A", A::from_f32(1.0))
            .collect_pixels(create_pixels, set_pixel)
    }

    /// Read only layers that contain rgb channels. Skips any other channels in the layer.
    ///
    /// Throws an error for images with deep data or subsampling.
    pub fn rgb_channels<R, G, B, Create, Set, Pixels>(
        self,
        create_pixels: Create,
        set_pixel: Set,
    ) -> CollectPixels<
        ReadRequiredChannel<ReadRequiredChannel<ReadRequiredChannel<NoneMore, R>, G>, B>,
        (R, G, B),
        Pixels,
        Create,
        Set,
    >
    where
        R: FromNativeSample,
        G: FromNativeSample,
        B: FromNativeSample,
        Create: Fn(Vec2<usize>, &RgbChannels) -> Pixels,
        Set: Fn(&mut Pixels, Vec2<usize>, (R, G, B)),
    {
        self.specific_channels()
            .required("R")
            .required("G")
            .required("B")
            .collect_pixels(create_pixels, set_pixel)
    }

    /// Read only layers that contain the specified channels.
    ///
    /// Further specify which channels by calling `.required("ChannelName")`
    /// or `.optional("ChannelName", default_value)`.
    /// Call `collect_pixels` afterwards to define the pixel container.
    ///
    /// Throws an error for images with deep data or subsampling.
    pub fn specific_channels(self) -> ReadZeroChannels {
        ReadZeroChannels {}
    }
}

/// Processes pixel blocks from a file and accumulates them into a single selected level.
///
/// Created by [`ReadSpecificLevel`] for each channel. Filters blocks to only read
/// the selected level and accumulates samples into the reader.
///
/// # Type Parameters
///
/// - `SamplesReader`: The underlying samples reader (e.g., `FlatSamplesReader`).
#[derive(Debug, Clone, PartialEq)]
pub struct SpecificLevelReader<SamplesReader> {
    /// The reader for the selected level's samples.
    reader: SamplesReader,

    /// The selected level index for filtering blocks.
    selected_level: Vec2<usize>,
}

/// Helper function to collect level information from a header.
///
/// Extracts all available resolution levels based on the block description
/// (singular, mipmap, or ripmap) and returns them as a `SmallVec` of [`LevelInfo`].
///
/// Used internally by [`ReadSpecificLevel`] to provide level information to the selector closure.
fn collect_level_info(
    header: &Header,
    channel: &ChannelDescription,
) -> SmallVec<[LevelInfo; 16]> {
    let data_size = header.layer_size / channel.sampling;
    let mut levels = SmallVec::new();

    if let crate::meta::BlockDescription::Tiles(tiles) = &header.blocks {
        match tiles.level_mode {
            LevelMode::Singular => {
                levels.push(LevelInfo {
                    index: Vec2(0, 0),
                    resolution: data_size,
                });
            }
            LevelMode::MipMap => {
                for (index, resolution) in mip_map_levels(tiles.rounding_mode, data_size) {
                    levels.push(LevelInfo {
                        index: Vec2(index, index),
                        resolution,
                    });
                }
            }
            LevelMode::RipMap => {
                for (index, resolution) in rip_map_levels(tiles.rounding_mode, data_size) {
                    levels.push(LevelInfo { index, resolution });
                }
            }
        }
    } else {
        // Scanline images have only one level
        levels.push(LevelInfo {
            index: Vec2(0, 0),
            resolution: data_size,
        });
    }

    levels
}

impl<S: ReadSamplesLevel, F> ReadSamples for ReadSpecificLevel<S, F>
where
    F: Fn(&[LevelInfo]) -> Vec2<usize>,
{
    type Reader = SpecificLevelReader<S::Reader>;

    fn create_sample_reader(
        &self,
        header: &Header,
        channel: &ChannelDescription,
    ) -> Result<Self::Reader> {
        // Collect available levels and let user select one
        let available_levels = collect_level_info(header, channel);
        let selected_level = (self.select_level)(&available_levels);

        // Find the resolution for the selected level
        let level_info = available_levels
            .iter()
            .find(|info| info.index == selected_level)
            .ok_or_else(|| {
                Error::invalid(format!(
                    "selected level {:?} not found in available levels {:?}",
                    selected_level,
                    available_levels.iter().map(|l| l.index).collect::<Vec<_>>()
                ))
            })?;

        let reader = self.read_samples.create_samples_level_reader(
            header,
            channel,
            selected_level,
            level_info.resolution,
        )?;

        Ok(SpecificLevelReader {
            reader,
            selected_level,
        })
    }
}

impl<S: SamplesReader> SamplesReader for SpecificLevelReader<S> {
    type Samples = S::Samples;

    fn filter_block(&self, tile: TileCoordinates) -> bool {
        // Only accept blocks from the selected level
        tile.level_index == self.selected_level
    }

    fn read_line(&mut self, line: LineRef<'_>) -> UnitResult {
        debug_assert_eq!(
            line.location.level, self.selected_level,
            "line from wrong level should have been filtered"
        );
        self.reader.read_line(line)
    }

    fn into_samples(self) -> Self::Samples {
        self.reader.into_samples()
    }
}

/// Processes pixel blocks from a file and accumulates them into multiple levels per channel.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct AllLevelsReader<SamplesReader> {
    levels: Levels<SamplesReader>,
}

/// A template that creates a [`SamplesReader`] once for each resolution level.
pub trait ReadSamplesLevel {
    /// The type of the temporary level reader
    type Reader: SamplesReader;

    /// Create a single reader for a single resolution level
    fn create_samples_level_reader(
        &self,
        header: &Header,
        channel: &ChannelDescription,
        level: Vec2<usize>,
        resolution: Vec2<usize>,
    ) -> Result<Self::Reader>;
}

impl<S: ReadSamplesLevel> ReadSamples for ReadAllLevels<S> {
    type Reader = AllLevelsReader<S::Reader>;

    fn create_sample_reader(
        &self,
        header: &Header,
        channel: &ChannelDescription,
    ) -> Result<Self::Reader> {
        let data_size = header.layer_size / channel.sampling;

        let levels = {
            if let crate::meta::BlockDescription::Tiles(tiles) = &header.blocks {
                match tiles.level_mode {
                    LevelMode::Singular => {
                        Levels::Singular(self.read_samples.create_samples_level_reader(
                            header,
                            channel,
                            Vec2(0, 0),
                            header.layer_size,
                        )?)
                    }

                    LevelMode::MipMap => Levels::Mip {
                        rounding_mode: tiles.rounding_mode,
                        level_data: {
                            let round = tiles.rounding_mode;
                            let maps: Result<LevelMaps<S::Reader>> =
                                mip_map_levels(round, data_size)
                                    .map(|(index, level_size)| {
                                        self.read_samples.create_samples_level_reader(
                                            header,
                                            channel,
                                            Vec2(index, index),
                                            level_size,
                                        )
                                    })
                                    .collect();

                            maps?
                        },
                    },

                    // TODO put this into Levels::new(..) ?
                    LevelMode::RipMap => Levels::Rip {
                        rounding_mode: tiles.rounding_mode,
                        level_data: {
                            let round = tiles.rounding_mode;
                            let level_count_x = compute_level_count(round, data_size.width());
                            let level_count_y = compute_level_count(round, data_size.height());
                            let maps: Result<LevelMaps<S::Reader>> =
                                rip_map_levels(round, data_size)
                                    .map(|(index, level_size)| {
                                        self.read_samples.create_samples_level_reader(
                                            header, channel, index, level_size,
                                        )
                                    })
                                    .collect();

                            RipMaps {
                                map_data: maps?,
                                level_count: Vec2(level_count_x, level_count_y),
                            }
                        },
                    },
                }
            }
            // scan line blocks never have mip maps
            else {
                Levels::Singular(self.read_samples.create_samples_level_reader(
                    header,
                    channel,
                    Vec2(0, 0),
                    data_size,
                )?)
            }
        };

        Ok(AllLevelsReader { levels })
    }
}

impl<S: SamplesReader> SamplesReader for AllLevelsReader<S> {
    type Samples = Levels<S::Samples>;

    fn filter_block(&self, _: TileCoordinates) -> bool {
        true
    }

    fn read_line(&mut self, line: LineRef<'_>) -> UnitResult {
        self.levels
            .get_level_mut(line.location.level)?
            .read_line(line)
    }

    fn into_samples(self) -> Self::Samples {
        match self.levels {
            Levels::Singular(level) => Levels::Singular(level.into_samples()),
            Levels::Mip {
                rounding_mode,
                level_data,
            } => Levels::Mip {
                rounding_mode,
                level_data: level_data.into_iter().map(|s| s.into_samples()).collect(),
            },

            Levels::Rip {
                rounding_mode,
                level_data,
            } => Levels::Rip {
                rounding_mode,
                level_data: RipMaps {
                    level_count: level_data.level_count,
                    map_data: level_data
                        .map_data
                        .into_iter()
                        .map(|s| s.into_samples())
                        .collect(),
                },
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[allow(unused_imports)]
    use crate::image::ChannelList;
    use crate::math::RoundingMode;

    /// Test LevelInfo struct creation and field access.
    #[test]
    fn level_info_creation() {
        let info = LevelInfo {
            index: Vec2(2, 2),
            resolution: Vec2(256, 256),
        };

        assert_eq!(info.index, Vec2(2, 2));
        assert_eq!(info.resolution, Vec2(256, 256));
    }

    /// Test LevelInfo comparison and equality.
    #[test]
    fn level_info_equality() {
        let a = LevelInfo {
            index: Vec2(1, 1),
            resolution: Vec2(512, 512),
        };
        let b = LevelInfo {
            index: Vec2(1, 1),
            resolution: Vec2(512, 512),
        };
        let c = LevelInfo {
            index: Vec2(2, 2),
            resolution: Vec2(256, 256),
        };

        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    /// Test LevelInfo copy semantics.
    #[test]
    fn level_info_copy() {
        let original = LevelInfo {
            index: Vec2(0, 0),
            resolution: Vec2(1024, 768),
        };
        let copy = original; // Copy
        assert_eq!(original, copy);
    }

    /// Test collect_level_info for scanline images (singular level).
    #[test]
    fn collect_level_info_scanline() {
        use crate::meta::header::Header;
        use crate::meta::attribute::{ChannelDescription, SampleType};

        // Create a minimal scanline header
        let mut header = Header::new(
            "test".into(),
            Vec2(1024, 768),
            smallvec![
                ChannelDescription::named("R", SampleType::F32)
            ],
        );
        header.blocks = crate::meta::BlockDescription::ScanLines;

        let channel = ChannelDescription::named("R", SampleType::F32);
        let levels = collect_level_info(&header, &channel);

        assert_eq!(levels.len(), 1);
        assert_eq!(levels[0].index, Vec2(0, 0));
        assert_eq!(levels[0].resolution, Vec2(1024, 768));
    }

    /// Test collect_level_info for singular tiled images.
    #[test]
    fn collect_level_info_singular_tiles() {
        use crate::meta::header::Header;
        use crate::meta::attribute::{ChannelDescription, SampleType, TileDescription};

        let mut header = Header::new(
            "test".into(),
            Vec2(512, 512),
            smallvec![
                ChannelDescription::named("R", SampleType::F32)
            ],
        );
        header.blocks = crate::meta::BlockDescription::Tiles(TileDescription {
            tile_size: Vec2(64, 64),
            level_mode: LevelMode::Singular,
            rounding_mode: RoundingMode::Down,
        });

        let channel = ChannelDescription::named("R", SampleType::F32);
        let levels = collect_level_info(&header, &channel);

        assert_eq!(levels.len(), 1);
        assert_eq!(levels[0].index, Vec2(0, 0));
        assert_eq!(levels[0].resolution, Vec2(512, 512));
    }

    /// Test collect_level_info for mipmap images.
    #[test]
    fn collect_level_info_mipmap() {
        use crate::meta::header::Header;
        use crate::meta::attribute::{ChannelDescription, SampleType, TileDescription};

        let mut header = Header::new(
            "test".into(),
            Vec2(256, 256),
            smallvec![
                ChannelDescription::named("R", SampleType::F32)
            ],
        );
        header.blocks = crate::meta::BlockDescription::Tiles(TileDescription {
            tile_size: Vec2(32, 32),
            level_mode: LevelMode::MipMap,
            rounding_mode: RoundingMode::Down,
        });

        let channel = ChannelDescription::named("R", SampleType::F32);
        let levels = collect_level_info(&header, &channel);

        // 256x256 mipmap has 9 levels: 256, 128, 64, 32, 16, 8, 4, 2, 1
        assert!(levels.len() >= 5);

        // Verify first few levels
        assert_eq!(levels[0].index, Vec2(0, 0));
        assert_eq!(levels[0].resolution, Vec2(256, 256));

        assert_eq!(levels[1].index, Vec2(1, 1));
        assert_eq!(levels[1].resolution, Vec2(128, 128));

        assert_eq!(levels[2].index, Vec2(2, 2));
        assert_eq!(levels[2].resolution, Vec2(64, 64));
    }

    /// Test level selection closure with find closest resolution.
    #[test]
    fn level_selector_find_closest() {
        let levels = vec![
            LevelInfo { index: Vec2(0, 0), resolution: Vec2(1024, 1024) },
            LevelInfo { index: Vec2(1, 1), resolution: Vec2(512, 512) },
            LevelInfo { index: Vec2(2, 2), resolution: Vec2(256, 256) },
            LevelInfo { index: Vec2(3, 3), resolution: Vec2(128, 128) },
        ];

        // Find level closest to 300x300
        let target = 300i64;
        let selected = levels
            .iter()
            .min_by_key(|info| {
                let dx = (info.resolution.x() as i64 - target).abs();
                let dy = (info.resolution.y() as i64 - target).abs();
                dx + dy
            })
            .map(|info| info.index)
            .unwrap();

        assert_eq!(selected, Vec2(2, 2)); // 256x256 is closest to 300x300
    }

    /// Test level selection by index.
    #[test]
    fn level_selector_by_index() {
        let levels = vec![
            LevelInfo { index: Vec2(0, 0), resolution: Vec2(512, 512) },
            LevelInfo { index: Vec2(1, 1), resolution: Vec2(256, 256) },
            LevelInfo { index: Vec2(2, 2), resolution: Vec2(128, 128) },
        ];

        // Select level 1 directly
        let selector = |_levels: &[LevelInfo]| Vec2(1, 1);
        let selected = selector(&levels);
        assert_eq!(selected, Vec2(1, 1));
    }

    /// Test level selection fallback for empty levels.
    #[test]
    fn level_selector_fallback() {
        let levels: Vec<LevelInfo> = vec![];

        // Fallback to level 0 if no levels available
        let selected = levels
            .iter()
            .min_by_key(|info| info.index.x())
            .map(|info| info.index)
            .unwrap_or(Vec2(0, 0));

        assert_eq!(selected, Vec2(0, 0));
    }

    /// Test ReadSpecificLevel debug formatting.
    #[test]
    fn read_specific_level_debug() {
        use crate::image::read::samples::ReadFlatSamples;

        let reader = ReadSpecificLevel {
            read_samples: ReadFlatSamples,
            select_level: |_: &[LevelInfo]| Vec2(0, 0),
        };

        let debug = format!("{:?}", reader);
        assert!(debug.contains("ReadSpecificLevel"));
        assert!(debug.contains("ReadFlatSamples"));
        assert!(debug.contains("<closure>"));
    }
}
