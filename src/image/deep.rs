//! Deep data storage for OpenEXR images with multiple samples per pixel.
//!
//! # Overview
//!
//! Deep images (introduced in OpenEXR 2.0) store a variable number of samples at each pixel,
//! unlike flat images which have exactly one sample per pixel. This is essential for:
//!
//! - **Volumetric effects**: Smoke, fog, clouds where rays pass through multiple density values
//! - **Particle systems**: Each particle contributes a sample at its projected pixel
//! - **Deep compositing**: Proper z-ordered layering without artifacts from pre-multiplied alpha
//! - **Hair/fur rendering**: Multiple transparent strands at each pixel
//!
//! # Architecture
//!
//! This module uses **Struct-of-Arrays (SoA)** layout for memory efficiency:
//! - All samples for channel 0 are stored contiguously, then all for channel 1, etc.
//! - This is cache-friendly when processing one channel at a time (common in compositing)
//! - OpenEXR file format also uses this layout, minimizing conversion overhead
//!
//! The alternative Array-of-Structs (AoS) layout would interleave channels per-sample,
//! which is less efficient for typical deep compositing workflows.
//!
//! # Sample Indexing
//!
//! [`DeepSamples`] uses cumulative offsets (prefix sums) for O(1) sample lookup:
//!
//! ```text
//! Pixel:           [A]    [B]    [C]    [D]
//! Sample counts:    2      0      3      1
//! Cumulative:       2      2      5      6  <- sample_offsets
//!
//! sample_count(A) = offsets[0] - 0 = 2
//! sample_count(B) = offsets[1] - offsets[0] = 0  
//! sample_count(C) = offsets[2] - offsets[1] = 3
//! sample_range(C) = (offsets[1], offsets[2]) = (2, 5)
//! ```
//!
//! This matches the OpenEXR file format's packed offset table representation.
//!
//! # Comparison with OpenEXR C++ API
//!
//! - C++: `DeepFrameBuffer` + `DeepScanLineInputFile::readPixels()`
//! - Rust: [`DeepSamples`] as unified in-memory representation
//!
//! The Rust API is simpler - no separate frame buffer setup, just read into [`DeepSamples`].
//!
//! # Usage
//!
//! Reading deep data:
//! ```ignore
//! let image = exr::image::read::deep::read_first_deep_layer_from_file("deep.exr")?;
//! let samples = &image.layer_data.channel_data.list[0].sample_data;
//!
//! for y in 0..samples.height {
//!     for x in 0..samples.width {
//!         let count = samples.sample_count(x, y);
//!         let (start, end) = samples.sample_range(y * samples.width + x);
//!         // Process samples[start..end] for this pixel
//!     }
//! }
//! ```
//!
//! # See Also
//!
//! - [`crate::image::read::deep`] - Reading deep images from files
//! - [`crate::image::write::deep`] - Writing deep images to files  
//! - [`crate::block::deep`] - Low-level block compression/decompression
//! - [OpenEXR Deep Data spec](https://openexr.com/en/latest/TechnicalIntroduction.html#deep-data)

use crate::error::{Error, Result};
use crate::meta::attribute::{ChannelList, SampleType};
use half::f16;

/// Deep samples storage using Struct-of-Arrays (SoA) layout.
///
/// # Design Rationale
///
/// This structure mirrors how OpenEXR stores deep data in files:
/// - **Packed sample data**: All samples stored contiguously without per-pixel gaps
/// - **Cumulative offsets**: O(1) lookup of sample range for any pixel
/// - **SoA layout**: Each channel's data in separate contiguous array
///
/// The cumulative offset approach (prefix sums) was chosen over per-pixel counts because:
/// 1. Direct range lookup: `samples[offsets[i-1]..offsets[i]]` vs computing prefix sum
/// 2. File format compatibility: OpenEXR stores cumulative counts, no conversion needed
/// 3. Memory efficiency: Same storage size, but O(1) vs O(n) for random access
///
/// # Memory Layout
///
/// Example for 2x2 image with per-pixel sample counts `[2, 0, 3, 1]`:
///
/// ```text
/// Pixels (row-major):  (0,0)  (1,0)  (0,1)  (1,1)
/// Per-pixel counts:      2      0      3      1
/// sample_offsets:       [2,     2,     5,     6]   <- cumulative sums
///
/// Channel data layout (6 total samples):
/// Index:    0    1    2    3    4    5
/// Pixel:  (0,0)(0,0)(0,1)(0,1)(0,1)(1,1)
///
/// Pixel (0,0): indices 0..2 (offsets[-1]=0 to offsets[0]=2)
/// Pixel (1,0): indices 2..2 (empty, offsets[0]=2 to offsets[1]=2)
/// Pixel (0,1): indices 2..5 (offsets[1]=2 to offsets[2]=5)
/// Pixel (1,1): indices 5..6 (offsets[2]=5 to offsets[3]=6)
/// ```
///
/// # Thread Safety
///
/// `DeepSamples` is `Send + Sync` when its channel data is. For parallel processing,
/// partition by pixel ranges and compute sample ranges from offsets.
///
/// # Performance Considerations
///
/// - **Sequential access**: Iterate pixels with [`pixels()`](Self::pixels) for cache efficiency
/// - **Random access**: Use [`sample_range()`](Self::sample_range) for O(1) lookup
/// - **Bulk operations**: Access channel data directly via `.channels[i]` for SIMD
#[derive(Debug, Clone, PartialEq)]
pub struct DeepSamples {
    /// Cumulative sample counts per pixel. Length = width * height.
    /// Value at index i = total samples for pixels 0 through i (inclusive).
    pub sample_offsets: Vec<u32>,

    /// Channel data in Struct-of-Arrays layout.
    pub channels: Vec<DeepChannelData>,

    /// Image width in pixels.
    pub width: usize,

    /// Image height in pixels.
    pub height: usize,
}

/// Sample data for a single channel in a deep image.
///
/// # Type Selection
///
/// OpenEXR supports three sample types, each with specific use cases:
///
/// | Type | Bits | Use Case |
/// |------|------|----------|
/// | `F16` | 16 | Color (RGBA), normals - good precision, half memory |
/// | `F32` | 32 | Depth (Z), HDR values needing full precision |
/// | `U32` | 32 | Object IDs, material indices, flags |
///
/// For deep compositing, typical channel types:
/// - `R`, `G`, `B`, `A` → F16 (color channels)
/// - `Z`, `ZBack` → F32 (depth requires precision for proper sorting)
///
/// # Memory Layout
///
/// All samples are stored contiguously. Use [`DeepSamples::sample_range()`] to find
/// the slice indices for a specific pixel's samples.
///
/// ```text
/// F16 channel: [sample0, sample1, sample2, ...] as Vec<f16>
/// F32 channel: [sample0, sample1, sample2, ...] as Vec<f32>
/// U32 channel: [sample0, sample1, sample2, ...] as Vec<u32>
/// ```
#[derive(Debug, Clone, PartialEq)]
pub enum DeepChannelData {
    /// 16-bit float samples.
    F16(Vec<f16>),

    /// 32-bit float samples.
    F32(Vec<f32>),

    /// 32-bit unsigned integer samples.
    U32(Vec<u32>),
}

/// Iterator over pixels in a deep image.
#[derive(Debug)]
pub struct DeepPixelIter<'a> {
    samples: &'a DeepSamples,
    index: usize,
}

/// Reference to samples at a single pixel.
#[derive(Debug)]
pub struct DeepPixelRef<'a> {
    /// Number of samples at this pixel.
    pub count: usize,

    /// Starting index in channel data arrays.
    pub start: usize,

    /// Ending index (exclusive) in channel data arrays.
    pub end: usize,

    /// Reference to channel data.
    pub channels: &'a [DeepChannelData],
}

impl DeepSamples {
    /// Create empty deep samples for given dimensions.
    pub fn new(width: usize, height: usize) -> Self {
        let pixel_count = width * height;
        Self {
            sample_offsets: vec![0; pixel_count],
            channels: Vec::new(),
            width,
            height,
        }
    }

    /// Create deep samples with pre-allocated capacity for sample offsets.
    pub fn with_capacity(width: usize, height: usize, _total_samples: usize) -> Self {
        let pixel_count = width * height;
        Self {
            sample_offsets: Vec::with_capacity(pixel_count),
            channels: Vec::new(),
            width,
            height,
        }
    }

    /// Total number of pixels.
    #[inline]
    pub fn pixel_count(&self) -> usize {
        self.width * self.height
    }

    /// Total number of samples across all pixels.
    #[inline]
    pub fn total_samples(&self) -> usize {
        self.sample_offsets.last().copied().unwrap_or(0) as usize
    }

    /// Get sample count at pixel (x, y).
    #[inline]
    pub fn sample_count(&self, x: usize, y: usize) -> usize {
        let idx = y * self.width + x;
        self.sample_count_at_index(idx)
    }

    /// Get sample count at linear pixel index.
    #[inline]
    pub fn sample_count_at_index(&self, idx: usize) -> usize {
        if idx >= self.sample_offsets.len() {
            return 0;
        }
        let end = self.sample_offsets[idx] as usize;
        let start = if idx == 0 {
            0
        } else {
            self.sample_offsets[idx - 1] as usize
        };
        end - start
    }

    /// Get sample range (start, end) for pixel at linear index.
    #[inline]
    pub fn sample_range(&self, idx: usize) -> (usize, usize) {
        if idx >= self.sample_offsets.len() {
            return (0, 0);
        }
        let end = self.sample_offsets[idx] as usize;
        let start = if idx == 0 {
            0
        } else {
            self.sample_offsets[idx - 1] as usize
        };
        (start, end)
    }

    /// Get reference to pixel samples at (x, y).
    pub fn pixel(&self, x: usize, y: usize) -> DeepPixelRef<'_> {
        let idx = y * self.width + x;
        let (start, end) = self.sample_range(idx);
        DeepPixelRef {
            count: end - start,
            start,
            end,
            channels: &self.channels,
        }
    }

    /// Iterate over all pixels.
    pub fn pixels(&self) -> DeepPixelIter<'_> {
        DeepPixelIter {
            samples: self,
            index: 0,
        }
    }

    /// Set sample offsets from cumulative counts.
    /// Counts must be monotonically non-decreasing.
    pub fn set_cumulative_counts(&mut self, counts: Vec<u32>) -> Result<()> {
        // Validate monotonic
        let mut prev = 0u32;
        for (i, &count) in counts.iter().enumerate() {
            if count < prev {
                return Err(Error::invalid(format!(
                    "sample counts not monotonic at {}: {} < {}",
                    i, count, prev
                )));
            }
            prev = count;
        }

        if counts.len() != self.pixel_count() {
            return Err(Error::invalid(format!(
                "sample count length {} != pixel count {}",
                counts.len(),
                self.pixel_count()
            )));
        }

        self.sample_offsets = counts;
        Ok(())
    }

    /// Allocate channel storage based on channel list.
    pub fn allocate_channels(&mut self, channel_list: &ChannelList) {
        let total = self.total_samples();
        self.channels.clear();
        self.channels.reserve(channel_list.list.len());

        for channel in &channel_list.list {
            let data = match channel.sample_type {
                SampleType::F16 => DeepChannelData::F16(vec![f16::ZERO; total]),
                SampleType::F32 => DeepChannelData::F32(vec![0.0f32; total]),
                SampleType::U32 => DeepChannelData::U32(vec![0u32; total]),
            };
            self.channels.push(data);
        }
    }

    /// Get maximum samples per pixel (for header attribute).
    pub fn max_samples_per_pixel(&self) -> u32 {
        let mut max = 0u32;
        let mut prev = 0u32;
        for &cum in &self.sample_offsets {
            let count = cum - prev;
            if count > max {
                max = count;
            }
            prev = cum;
        }
        max
    }

    /// Validate that channel data lengths match total_samples.
    pub fn validate(&self) -> Result<()> {
        let total = self.total_samples();

        for (i, channel) in self.channels.iter().enumerate() {
            let len = channel.len();
            if len != total {
                return Err(Error::invalid(format!(
                    "channel {} has {} samples, expected {}",
                    i, len, total
                )));
            }
        }

        Ok(())
    }
}

impl DeepChannelData {
    /// Number of samples in this channel.
    #[inline]
    pub fn len(&self) -> usize {
        match self {
            DeepChannelData::F16(v) => v.len(),
            DeepChannelData::F32(v) => v.len(),
            DeepChannelData::U32(v) => v.len(),
        }
    }

    /// Check if channel is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get sample type of this channel.
    pub fn sample_type(&self) -> SampleType {
        match self {
            DeepChannelData::F16(_) => SampleType::F16,
            DeepChannelData::F32(_) => SampleType::F32,
            DeepChannelData::U32(_) => SampleType::U32,
        }
    }

    /// Get f16 value at index (panics if wrong type).
    #[inline]
    pub fn get_f16(&self, index: usize) -> f16 {
        match self {
            DeepChannelData::F16(v) => v[index],
            _ => panic!("channel is not F16"),
        }
    }

    /// Get f32 value at index (panics if wrong type).
    #[inline]
    pub fn get_f32(&self, index: usize) -> f32 {
        match self {
            DeepChannelData::F32(v) => v[index],
            _ => panic!("channel is not F32"),
        }
    }

    /// Get u32 value at index (panics if wrong type).
    #[inline]
    pub fn get_u32(&self, index: usize) -> u32 {
        match self {
            DeepChannelData::U32(v) => v[index],
            _ => panic!("channel is not U32"),
        }
    }

    /// Get mutable f16 slice.
    pub fn as_f16_mut(&mut self) -> Option<&mut Vec<f16>> {
        match self {
            DeepChannelData::F16(v) => Some(v),
            _ => None,
        }
    }

    /// Get mutable f32 slice.
    pub fn as_f32_mut(&mut self) -> Option<&mut Vec<f32>> {
        match self {
            DeepChannelData::F32(v) => Some(v),
            _ => None,
        }
    }

    /// Get mutable u32 slice.
    pub fn as_u32_mut(&mut self) -> Option<&mut Vec<u32>> {
        match self {
            DeepChannelData::U32(v) => Some(v),
            _ => None,
        }
    }

    /// Bytes per sample element.
    pub fn bytes_per_sample(&self) -> usize {
        match self {
            DeepChannelData::F16(_) => 2,
            DeepChannelData::F32(_) => 4,
            DeepChannelData::U32(_) => 4,
        }
    }
}

impl<'a> Iterator for DeepPixelIter<'a> {
    type Item = DeepPixelRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.samples.pixel_count() {
            return None;
        }

        let (start, end) = self.samples.sample_range(self.index);
        self.index += 1;

        Some(DeepPixelRef {
            count: end - start,
            start,
            end,
            channels: &self.samples.channels,
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.samples.pixel_count() - self.index;
        (remaining, Some(remaining))
    }
}

impl<'a> ExactSizeIterator for DeepPixelIter<'a> {}

impl<'a> DeepPixelRef<'a> {
    /// Get f16 sample at given sample index within this pixel.
    pub fn get_f16(&self, channel: usize, sample: usize) -> f16 {
        self.channels[channel].get_f16(self.start + sample)
    }

    /// Get f32 sample at given sample index within this pixel.
    pub fn get_f32(&self, channel: usize, sample: usize) -> f32 {
        self.channels[channel].get_f32(self.start + sample)
    }

    /// Get u32 sample at given sample index within this pixel.
    pub fn get_u32(&self, channel: usize, sample: usize) -> u32 {
        self.channels[channel].get_u32(self.start + sample)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn deep_samples_basic() {
        let mut samples = DeepSamples::new(2, 2);

        // Set cumulative counts: [2, 2, 5, 6]
        // Means: pixel 0 has 2, pixel 1 has 0, pixel 2 has 3, pixel 3 has 1
        samples.set_cumulative_counts(vec![2, 2, 5, 6]).unwrap();

        assert_eq!(samples.total_samples(), 6);
        assert_eq!(samples.sample_count(0, 0), 2);
        assert_eq!(samples.sample_count(1, 0), 0);
        assert_eq!(samples.sample_count(0, 1), 3);
        assert_eq!(samples.sample_count(1, 1), 1);
        assert_eq!(samples.max_samples_per_pixel(), 3);
    }

    #[test]
    fn deep_samples_iteration() {
        let mut samples = DeepSamples::new(2, 2);
        samples.set_cumulative_counts(vec![1, 3, 3, 5]).unwrap();

        let counts: Vec<_> = samples.pixels().map(|p| p.count).collect();
        assert_eq!(counts, vec![1, 2, 0, 2]);
    }

    #[test]
    fn deep_channel_data() {
        let mut channel = DeepChannelData::F32(vec![1.0, 2.0, 3.0]);
        assert_eq!(channel.len(), 3);
        assert_eq!(channel.sample_type(), SampleType::F32);
        assert_eq!(channel.get_f32(1), 2.0);

        if let Some(v) = channel.as_f32_mut() {
            v[0] = 10.0;
        }
        assert_eq!(channel.get_f32(0), 10.0);
    }

    #[test]
    fn deep_samples_validation() {
        let mut samples = DeepSamples::new(2, 2);
        samples.set_cumulative_counts(vec![2, 4, 6, 8]).unwrap();

        // Wrong channel length should fail validation
        samples.channels.push(DeepChannelData::F32(vec![0.0; 5])); // Wrong: should be 8
        assert!(samples.validate().is_err());

        // Correct length should pass
        samples.channels[0] = DeepChannelData::F32(vec![0.0; 8]);
        assert!(samples.validate().is_ok());
    }

    #[test]
    fn cumulative_counts_must_be_monotonic() {
        let mut samples = DeepSamples::new(2, 2);
        // Non-monotonic should fail
        assert!(samples.set_cumulative_counts(vec![2, 1, 3, 4]).is_err());
    }

    // === Edge Case Tests (7.3) ===

    #[test]
    fn edge_case_all_pixels_zero_samples() {
        // All pixels have 0 samples - valid edge case for sparse deep images
        let mut samples = DeepSamples::new(3, 3);
        samples
            .set_cumulative_counts(vec![0, 0, 0, 0, 0, 0, 0, 0, 0])
            .unwrap();

        assert_eq!(samples.total_samples(), 0);
        assert_eq!(samples.max_samples_per_pixel(), 0);

        for y in 0..3 {
            for x in 0..3 {
                assert_eq!(samples.sample_count(x, y), 0);
                let (start, end) = samples.sample_range(y * 3 + x);
                assert_eq!(start, end); // Empty range
            }
        }

        // Iteration should work with all zero-count pixels
        let counts: Vec<_> = samples.pixels().map(|p| p.count).collect();
        assert_eq!(counts, vec![0; 9]);
    }

    #[test]
    fn edge_case_single_sample_per_pixel() {
        // Each pixel has exactly 1 sample
        let mut samples = DeepSamples::new(2, 2);
        samples.set_cumulative_counts(vec![1, 2, 3, 4]).unwrap();

        assert_eq!(samples.total_samples(), 4);
        assert_eq!(samples.max_samples_per_pixel(), 1);

        // Add channel data
        samples
            .channels
            .push(DeepChannelData::F32(vec![1.0, 2.0, 3.0, 4.0]));

        // Each pixel should have exactly one sample
        for (i, pixel) in samples.pixels().enumerate() {
            assert_eq!(pixel.count, 1);
            assert_eq!(pixel.get_f32(0, 0), (i + 1) as f32);
        }
    }

    #[test]
    fn edge_case_mixed_zero_and_nonzero() {
        // Mix of pixels with 0, 1, and many samples
        let mut samples = DeepSamples::new(4, 1);
        // Pixel 0: 0 samples, Pixel 1: 5 samples, Pixel 2: 0 samples, Pixel 3: 1 sample
        samples.set_cumulative_counts(vec![0, 5, 5, 6]).unwrap();

        assert_eq!(samples.sample_count(0, 0), 0);
        assert_eq!(samples.sample_count(1, 0), 5);
        assert_eq!(samples.sample_count(2, 0), 0);
        assert_eq!(samples.sample_count(3, 0), 1);

        assert_eq!(samples.total_samples(), 6);
        assert_eq!(samples.max_samples_per_pixel(), 5);
    }

    #[test]
    fn edge_case_high_sample_count() {
        // Single pixel with many samples (stress test for max_samples)
        let mut samples = DeepSamples::new(1, 1);
        let high_count = 10_000u32;
        samples.set_cumulative_counts(vec![high_count]).unwrap();

        assert_eq!(samples.total_samples(), high_count as usize);
        assert_eq!(samples.max_samples_per_pixel(), high_count);
        assert_eq!(samples.sample_count(0, 0), high_count as usize);

        let (start, end) = samples.sample_range(0);
        assert_eq!(start, 0);
        assert_eq!(end, high_count as usize);
    }

    #[test]
    fn edge_case_large_image_dimensions() {
        // Large image with sparse samples
        let width = 1000;
        let height = 1000;
        let mut samples = DeepSamples::new(width, height);

        // Cumulative counts: only corners have samples
        // top-left (0): 1 sample -> cumulative 1
        // top-right (999): 1 sample -> cumulative 2
        // bottom-left (999000): 2 samples -> cumulative 4
        // bottom-right (999999): 4 samples -> cumulative 8
        let mut counts = vec![0u32; width * height];

        // Build cumulative: first pixel = 1, rest of first row = 1, until top-right = 2
        counts[0] = 1;
        for i in 1..width - 1 {
            counts[i] = 1;
        }
        counts[width - 1] = 2;

        // Middle rows all have cumulative = 2
        for i in width..(height - 1) * width {
            counts[i] = 2;
        }

        // Last row: bottom-left has 2 more samples (cumulative 4)
        counts[(height - 1) * width] = 4;
        for i in (height - 1) * width + 1..width * height - 1 {
            counts[i] = 4;
        }
        counts[width * height - 1] = 8; // bottom-right: 4 more samples

        samples.set_cumulative_counts(counts).unwrap();

        assert_eq!(samples.total_samples(), 8);
        assert_eq!(samples.sample_count(0, 0), 1);
        assert_eq!(samples.sample_count(width - 1, 0), 1); // 2 - 1 = 1
        assert_eq!(samples.sample_count(0, height - 1), 2); // 4 - 2 = 2
        assert_eq!(samples.sample_count(width - 1, height - 1), 4); // 8 - 4 = 4
        assert_eq!(samples.max_samples_per_pixel(), 4);
    }

    #[test]
    fn edge_case_single_pixel_image() {
        // Minimal 1x1 image
        let mut samples = DeepSamples::new(1, 1);
        samples.set_cumulative_counts(vec![3]).unwrap();

        assert_eq!(samples.pixel_count(), 1);
        assert_eq!(samples.total_samples(), 3);
        assert_eq!(samples.sample_count(0, 0), 3);

        samples
            .channels
            .push(DeepChannelData::F16(vec![f16::from_f32(1.0); 3]));
        assert!(samples.validate().is_ok());
    }

    #[test]
    fn edge_case_wide_single_row() {
        // Very wide single-row image (common for scanline processing)
        let width = 4096;
        let mut samples = DeepSamples::new(width, 1);

        // Alternating 0 and 1 samples
        let counts: Vec<u32> = (0..width as u32).map(|i| (i + 1) / 2).collect();
        samples.set_cumulative_counts(counts).unwrap();

        assert_eq!(samples.total_samples(), width / 2);
        assert_eq!(samples.sample_count(0, 0), 0);
        assert_eq!(samples.sample_count(1, 0), 1);
        assert_eq!(samples.sample_count(2, 0), 0);
        assert_eq!(samples.sample_count(3, 0), 1);
    }
}
