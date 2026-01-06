//! How to read samples (a grid of `f32`, `f16` or `u32` values).

use crate::block::chunk::TileCoordinates;
use crate::block::lines::LineRef;
use crate::error::{Result, UnitResult};
use crate::image::read::any_channels::{ReadSamples, SamplesReader};
use crate::image::read::levels::{
    LevelInfo, ReadAllLevels, ReadLargestLevel, ReadSamplesLevel, ReadSpecificLevel,
};
use crate::image::*;
use crate::math::Vec2;
use crate::meta::attribute::{ChannelDescription, SampleType};
use crate::meta::header::Header;
// use crate::image::read::layers::ReadChannels;

/// Specify to read only flat samples and no "deep data".
/// Note: For deep data, use `read().deep_data()` instead.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct ReadFlatSamples;
// pub struct ReadAnySamples;

impl ReadFlatSamples {
    // TODO
    // e. g. `let sum = reader.any_channels_with(|sample, sum| sum += sample)`
    // pub fn any_channels_with <S> (self, storage: S) -> {  }

    /// Specify to read only the highest resolution level, skipping all smaller variations.
    pub fn largest_resolution_level(self) -> ReadLargestLevel<Self> {
        ReadLargestLevel { read_samples: self }
    }

    /// Specify to read all contained resolution levels from the image, if any.
    pub fn all_resolution_levels(self) -> ReadAllLevels<Self> {
        ReadAllLevels { read_samples: self }
    }

    /// Select a specific resolution level using a user-provided closure.
    ///
    /// This is useful for LOD (Level of Detail) systems where you want to read
    /// a specific mipmap level based on viewing distance, texture budget, or other criteria.
    ///
    /// The closure receives a slice of [`LevelInfo`] describing all available levels
    /// (resolutions and their indices) and must return the `Vec2<usize>` index of the
    /// desired level to read.
    ///
    /// # Arguments
    ///
    /// * `select_level` - A closure that receives available level information and returns
    ///   the index of the level to read. For mipmaps, return `Vec2(n, n)` where `n` is
    ///   the mip level (0 = full resolution). For ripmaps, X and Y indices can differ.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use exrs::prelude::*;
    ///
    /// // Read mipmap level 1 (half resolution)
    /// let image = read()
    ///     .no_deep_data()
    ///     .specific_resolution_level(|_| Vec2(1, 1))
    ///     .all_channels()
    ///     .first_valid_layer()
    ///     .from_file("mipmapped.exr")?;
    ///
    /// // Read the level closest to 512x512
    /// let image = read()
    ///     .no_deep_data()
    ///     .specific_resolution_level(|levels| {
    ///         levels.iter()
    ///             .min_by_key(|info| {
    ///                 let dx = (info.resolution.x() as i64 - 512).abs();
    ///                 let dy = (info.resolution.y() as i64 - 512).abs();
    ///                 dx + dy
    ///             })
    ///             .map(|info| info.index)
    ///             .unwrap_or(Vec2(0, 0))
    ///     })
    ///     .all_channels()
    ///     .first_valid_layer()
    ///     .from_file("mipmapped.exr")?;
    /// ```
    ///
    /// # See Also
    ///
    /// * [`largest_resolution_level`](Self::largest_resolution_level) - Always read level 0.
    /// * [`all_resolution_levels`](Self::all_resolution_levels) - Read all levels.
    /// * [`LevelInfo`] - Information about each available level.
    ///
    /// Implements: DEAD_CODE_ANALYSIS.md item #9
    pub fn specific_resolution_level<F>(self, select_level: F) -> ReadSpecificLevel<Self, F>
    where
        F: Fn(&[LevelInfo]) -> Vec2<usize>,
    {
        ReadSpecificLevel {
            read_samples: self,
            select_level,
        }
    }
}

// Unified deep/flat reader is available via `read().flat_and_deep_data()...`
// or the convenience function `read_first_any_layer_from_file()`.
// For direct access to types, use `crate::image::read::any_samples`.

/// Processes pixel blocks from a file and accumulates them into a grid of samples, for example "Red" or "Alpha".
#[derive(Debug, Clone, PartialEq)]
pub struct FlatSamplesReader {
    level: Vec2<usize>,
    resolution: Vec2<usize>,
    samples: FlatSamples,
}

// only used when samples is directly inside a channel, without levels
impl ReadSamples for ReadFlatSamples {
    type Reader = FlatSamplesReader;

    fn create_sample_reader(
        &self,
        header: &Header,
        channel: &ChannelDescription,
    ) -> Result<Self::Reader> {
        self.create_samples_level_reader(header, channel, Vec2(0, 0), header.layer_size)
    }
}

impl ReadSamplesLevel for ReadFlatSamples {
    type Reader = FlatSamplesReader;

    fn create_samples_level_reader(
        &self,
        _header: &Header,
        channel: &ChannelDescription,
        level: Vec2<usize>,
        resolution: Vec2<usize>,
    ) -> Result<Self::Reader> {
        Ok(FlatSamplesReader {
            level,
            resolution, // TODO sampling
            samples: match channel.sample_type {
                SampleType::F16 => FlatSamples::F16(vec![f16::ZERO; resolution.area()]),
                SampleType::F32 => FlatSamples::F32(vec![0.0; resolution.area()]),
                SampleType::U32 => FlatSamples::U32(vec![0; resolution.area()]),
            },
        })
    }
}

impl SamplesReader for FlatSamplesReader {
    type Samples = FlatSamples;

    fn filter_block(&self, tile: TileCoordinates) -> bool {
        tile.level_index == self.level
    }

    fn read_line(&mut self, line: LineRef<'_>) -> UnitResult {
        let index = line.location;
        let resolution = self.resolution;

        // the index is generated by ourselves and must always be correct
        debug_assert_eq!(index.level, self.level, "line should have been filtered");
        debug_assert!(
            index.position.x() + index.sample_count <= resolution.width(),
            "line index calculation bug"
        );
        debug_assert!(
            index.position.y() < resolution.height(),
            "line index calculation bug"
        );
        debug_assert_ne!(resolution.0, 0, "sample size bug");

        let start_index = index.position.y() * resolution.width() + index.position.x();
        let end_index = start_index + index.sample_count;

        debug_assert!(
            start_index < end_index && end_index <= self.samples.len(),
            "for resolution {:?}, this is an invalid line: {:?}",
            self.resolution,
            line.location
        );

        match &mut self.samples {
            FlatSamples::F16(samples) => line
                .read_samples_into_slice(&mut samples[start_index..end_index])
                .expect("writing line bytes failed"),

            FlatSamples::F32(samples) => line
                .read_samples_into_slice(&mut samples[start_index..end_index])
                .expect("writing line bytes failed"),

            FlatSamples::U32(samples) => line
                .read_samples_into_slice(&mut samples[start_index..end_index])
                .expect("writing line bytes failed"),
        }

        Ok(())
    }

    fn into_samples(self) -> FlatSamples {
        self.samples
    }
}
