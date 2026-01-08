//! Worker thread handler for image processing.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender};

use egui::Color32;

use crate::image::read::deep::read_first_deep_layer_from_file;
use crate::image::Layers;
use crate::prelude::*;
use crate::view::messages::{Generation, ViewerEvent, ViewerMsg};
use crate::view::state::{ChannelMode, DeepMode, DepthMode, View3DMode};

/// Loaded image data.
enum LoadedImage {
    Flat(Image<Layers<AnyChannels<FlatSamples>>>),
    Deep(crate::image::write::deep::DeepImage),
}

/// Worker thread handler.
pub struct ViewerHandler {
    rx: Receiver<ViewerMsg>,
    tx: Sender<ViewerEvent>,

    generation: Generation,
    image: Option<LoadedImage>,
    image_path: Option<PathBuf>,

    // Settings
    current_layer: String,
    current_channel: String,
    channel_mode: ChannelMode,
    deep_mode: DeepMode,
    depth_mode: DepthMode,
    exposure: f32,
    apply_srgb: bool,
    depth_near: f32,
    depth_far: f32,
    depth_invert: bool,
    slice_near: f32,
    slice_far: f32,

    // View
    zoom: f32,
    pan: [f32; 2],
    viewport: [f32; 2],
    
    // 3D settings
    view_3d_mode: View3DMode,

    verbose: u8,
}

impl ViewerHandler {
    pub fn new(rx: Receiver<ViewerMsg>, tx: Sender<ViewerEvent>, verbose: u8) -> Self {
        Self {
            rx,
            tx,
            generation: 0,
            image: None,
            image_path: None,
            current_layer: String::new(),
            current_channel: String::new(),
            channel_mode: ChannelMode::Color,
            deep_mode: DeepMode::Flattened,
            depth_mode: DepthMode::AutoNormalize,
            exposure: 0.0,
            apply_srgb: true,
            depth_near: 0.0,
            depth_far: 1.0,
            depth_invert: false,
            slice_near: 0.0,
            slice_far: 1.0,
            zoom: 1.0,
            pan: [0.0, 0.0],
            viewport: [1280.0, 720.0],
            view_3d_mode: View3DMode::Heightfield,
            verbose,
        }
    }

    pub fn run(mut self) {
        while let Ok(msg) = self.rx.recv() {
            match msg {
                ViewerMsg::Close => break,
                ViewerMsg::SyncGeneration(g) => self.generation = g,
                ViewerMsg::LoadImage(path) => self.load_image(path),
                ViewerMsg::SetLayer(layer) => {
                    self.current_layer = layer;
                    self.regenerate();
                }
                ViewerMsg::SetChannel(ch) => {
                    self.current_channel = ch;
                    self.regenerate();
                }
                ViewerMsg::SetChannelMode(mode) => {
                    self.channel_mode = mode;
                    self.regenerate();
                }
                ViewerMsg::SetDeepMode(mode) => {
                    self.deep_mode = mode;
                    self.regenerate();
                }
                ViewerMsg::SetDepthMode(mode) => {
                    self.depth_mode = mode;
                    self.regenerate();
                }
                ViewerMsg::SetDepthRange(near, far) => {
                    self.depth_near = near;
                    self.depth_far = far;
                    self.regenerate();
                }
                ViewerMsg::SetSliceRange(near, far) => {
                    self.slice_near = near;
                    self.slice_far = far;
                    self.regenerate();
                }
                ViewerMsg::SetExposure(ev) => {
                    self.exposure = ev;
                    self.regenerate();
                }
                ViewerMsg::SetSrgb(v) => {
                    self.apply_srgb = v;
                    self.regenerate();
                }
                ViewerMsg::SetInvertDepth(v) => {
                    self.depth_invert = v;
                    self.regenerate();
                }
                ViewerMsg::Regenerate => self.regenerate(),
                ViewerMsg::Zoom { factor } => self.zoom(factor),
                ViewerMsg::Pan { delta } => self.pan(delta),
                ViewerMsg::FitToWindow => self.fit_to_window(),
                ViewerMsg::Home => self.home(),
                ViewerMsg::SetViewport(size) => self.viewport = size,
                ViewerMsg::Request3DData => self.send_3d_data(),
                ViewerMsg::Set3DMode(mode) => {
                    self.view_3d_mode = mode;
                    self.send_3d_data();
                }
                // UI-only messages - handled in UI thread
                ViewerMsg::SetPointSize(_) | ViewerMsg::Reset3DCamera | ViewerMsg::Toggle3D(_) => {}
            }
        }

        if self.verbose > 0 {
            eprintln!("[viewer] Handler shutdown");
        }
    }

    fn send(&self, event: ViewerEvent) {
        let _ = self.tx.send(event);
    }

    fn log(&self, msg: &str) {
        if self.verbose > 0 {
            eprintln!("[viewer] {msg}");
        }
    }

    fn load_image(&mut self, path: PathBuf) {
        self.log(&format!("Loading: {}", path.display()));

        // Try deep first, then flat
        let result = read_first_deep_layer_from_file(&path)
            .map(LoadedImage::Deep)
            .or_else(|_| {
                read()
                    .no_deep_data()
                    .largest_resolution_level()
                    .all_channels()
                    .all_layers()
                    .all_attributes()
                    .from_file(&path)
                    .map(LoadedImage::Flat)
            });

        match result {
            Ok(img) => {
                let (dims, layers, channels, is_deep, total_samples, depth_range) = match &img {
                    LoadedImage::Flat(flat) => {
                        let layer = flat.layer_data.first();
                        let dims = layer.map(|l| (l.size.x(), l.size.y())).unwrap_or((0, 0));
                        let layers: Vec<String> = flat
                            .layer_data
                            .iter()
                            .map(|l| {
                                l.attributes
                                    .layer_name
                                    .as_ref()
                                    .map(|t| t.to_string())
                                    .unwrap_or_else(|| "default".into())
                            })
                            .collect();
                        let channels: Vec<String> = layer
                            .map(|l| {
                                l.channel_data
                                    .list
                                    .iter()
                                    .map(|c| c.name.to_string())
                                    .collect()
                            })
                            .unwrap_or_default();
                        
                        // Find Z range
                        let depth_range = self.find_depth_range_flat(layer);
                        
                        (dims, layers, channels, false, 0, depth_range)
                    }
                    LoadedImage::Deep(deep) => {
                        let layer = &deep.layer_data;
                        let dims = (layer.size.x(), layer.size.y());
                        let layers = vec![layer
                            .attributes
                            .layer_name
                            .as_ref()
                            .map(|t| t.to_string())
                            .unwrap_or_else(|| "deep".into())];
                        let channels: Vec<String> = layer
                            .channel_data
                            .list
                            .iter()
                            .map(|c| c.name.to_string())
                            .collect();
                        
                        // Get sample info from first channel's DeepSamples
                        let total = layer
                            .channel_data
                            .list
                            .first()
                            .map(|c| c.sample_data.total_samples())
                            .unwrap_or(0);
                        
                        // Find Z range in deep data
                        let depth_range = self.find_depth_range_deep(&layer.channel_data);
                        
                        (dims, layers, channels, true, total, depth_range)
                    }
                };

                self.image = Some(img);
                self.image_path = Some(path.clone());

                if let Some(first) = layers.first() {
                    self.current_layer = first.clone();
                }

                if let Some((min, max)) = depth_range {
                    self.depth_near = min;
                    self.depth_far = max;
                    self.slice_near = min;
                    self.slice_far = max;
                }

                self.send(ViewerEvent::ImageLoaded {
                    path,
                    dims,
                    layers,
                    channels,
                    is_deep,
                    total_samples,
                    depth_range,
                });

                self.regenerate();
            }
            Err(e) => {
                self.send(ViewerEvent::Error(format!("Failed to load: {e}")));
            }
        }
    }

    fn find_depth_range_flat(
        &self,
        layer: Option<&Layer<AnyChannels<FlatSamples>>>,
    ) -> Option<(f32, f32)> {
        let layer = layer?;
        let z_channel = layer
            .channel_data
            .list
            .iter()
            .find(|c| c.name.to_string() == "Z" || c.name.to_string() == "depth")?;

        let mut min = f32::MAX;
        let mut max = f32::MIN;

        match &z_channel.sample_data {
            FlatSamples::F32(data) => {
                for &v in data.iter() {
                    if v.is_finite() {
                        min = min.min(v);
                        max = max.max(v);
                    }
                }
            }
            FlatSamples::F16(data) => {
                for &v in data.iter() {
                    let f = v.to_f32();
                    if f.is_finite() {
                        min = min.min(f);
                        max = max.max(f);
                    }
                }
            }
            _ => return None,
        }

        if min < max {
            Some((min, max))
        } else {
            None
        }
    }

    fn find_depth_range_deep(
        &self,
        channels: &AnyChannels<crate::image::deep::DeepSamples>,
    ) -> Option<(f32, f32)> {
        // Find Z channel in deep data
        let first_ch = channels.list.first()?;
        let samples = &first_ch.sample_data;

        // Look for Z in channel names
        let z_idx = channels
            .list
            .iter()
            .position(|c| c.name.to_string() == "Z")?;

        if z_idx >= samples.channels.len() {
            return None;
        }

        let mut min = f32::MAX;
        let mut max = f32::MIN;

        match &samples.channels[z_idx] {
            crate::image::deep::DeepChannelData::F32(data) => {
                for &v in data.iter() {
                    if v.is_finite() {
                        min = min.min(v);
                        max = max.max(v);
                    }
                }
            }
            crate::image::deep::DeepChannelData::F16(data) => {
                for &v in data.iter() {
                    let f = v.to_f32();
                    if f.is_finite() {
                        min = min.min(f);
                        max = max.max(f);
                    }
                }
            }
            _ => return None,
        }

        if min < max {
            Some((min, max))
        } else {
            None
        }
    }

    fn regenerate(&mut self) {
        let Some(image) = &self.image else { return };

        let pixels = match image {
            LoadedImage::Flat(flat) => self.render_flat(flat),
            LoadedImage::Deep(deep) => self.render_deep(deep),
        };

        let (width, height) = match image {
            LoadedImage::Flat(f) => f
                .layer_data
                .first()
                .map(|l| (l.size.x(), l.size.y()))
                .unwrap_or((0, 0)),
            LoadedImage::Deep(d) => (d.layer_data.size.x(), d.layer_data.size.y()),
        };

        self.send(ViewerEvent::TextureReady {
            generation: self.generation,
            width,
            height,
            pixels,
        });
    }

    fn render_flat(&self, image: &Image<Layers<AnyChannels<FlatSamples>>>) -> Vec<Color32> {
        let layer = match image.layer_data.first() {
            Some(l) => l,
            None => return Vec::new(),
        };

        let (w, h) = (layer.size.x(), layer.size.y());
        let pixel_count = w * h;

        // Find channels
        let find_ch = |name: &str| {
            layer
                .channel_data
                .list
                .iter()
                .find(|c| c.name.to_string() == name)
        };

        let r_ch = find_ch("R");
        let g_ch = find_ch("G");
        let b_ch = find_ch("B");
        let a_ch = find_ch("A");
        let z_ch = find_ch("Z").or_else(|| find_ch("depth"));

        // Extract data as f32
        let get_f32 = |ch: Option<&AnyChannel<FlatSamples>>| -> Vec<f32> {
            ch.map(|c| match &c.sample_data {
                FlatSamples::F32(d) => d.clone(),
                FlatSamples::F16(d) => d.iter().map(|v| v.to_f32()).collect(),
                FlatSamples::U32(d) => d.iter().map(|&v| v as f32 / u32::MAX as f32).collect(),
            })
            .unwrap_or_else(|| vec![0.0; pixel_count])
        };

        let r = get_f32(r_ch);
        let g = get_f32(g_ch);
        let b = get_f32(b_ch);
        let a = get_f32(a_ch);
        let z = get_f32(z_ch);

        let exp_mult = 2.0_f32.powf(self.exposure);

        (0..pixel_count)
            .map(|i| {
                let (mut rv, mut gv, mut bv) = match self.channel_mode {
                    ChannelMode::Color => (r[i], g[i], b[i]),
                    ChannelMode::Red => (r[i], r[i], r[i]),
                    ChannelMode::Green => (g[i], g[i], g[i]),
                    ChannelMode::Blue => (b[i], b[i], b[i]),
                    ChannelMode::Alpha => (a[i], a[i], a[i]),
                    ChannelMode::Depth => {
                        let d = self.normalize_depth(z[i]);
                        (d, d, d)
                    }
                    ChannelMode::Luminance => {
                        let l = 0.2126 * r[i] + 0.7152 * g[i] + 0.0722 * b[i];
                        (l, l, l)
                    }
                    ChannelMode::Custom(idx) => {
                        if let Some(ch) = layer.channel_data.list.get(idx) {
                            let v = match &ch.sample_data {
                                FlatSamples::F32(d) => d.get(i).copied().unwrap_or(0.0),
                                FlatSamples::F16(d) => {
                                    d.get(i).map(|v| v.to_f32()).unwrap_or(0.0)
                                }
                                FlatSamples::U32(d) => {
                                    d.get(i).map(|&v| v as f32 / u32::MAX as f32).unwrap_or(0.0)
                                }
                            };
                            (v, v, v)
                        } else {
                            (0.0, 0.0, 0.0)
                        }
                    }
                };

                // Apply exposure
                rv *= exp_mult;
                gv *= exp_mult;
                bv *= exp_mult;

                // Apply sRGB gamma
                if self.apply_srgb {
                    rv = linear_to_srgb(rv);
                    gv = linear_to_srgb(gv);
                    bv = linear_to_srgb(bv);
                }

                // Clamp and convert
                let rb = (rv.clamp(0.0, 1.0) * 255.0) as u8;
                let gb = (gv.clamp(0.0, 1.0) * 255.0) as u8;
                let bb = (bv.clamp(0.0, 1.0) * 255.0) as u8;

                Color32::from_rgb(rb, gb, bb)
            })
            .collect()
    }

    fn render_deep(&self, image: &crate::image::write::deep::DeepImage) -> Vec<Color32> {
        let layer = &image.layer_data;
        let (w, h) = (layer.size.x(), layer.size.y());
        let pixel_count = w * h;

        // Get first channel's DeepSamples (contains all data)
        let samples = match layer.channel_data.list.first() {
            Some(ch) => &ch.sample_data,
            None => return vec![Color32::BLACK; pixel_count],
        };

        // Find channel indices
        let find_idx = |name: &str| {
            layer
                .channel_data
                .list
                .iter()
                .position(|c| c.name.to_string() == name)
        };

        let r_idx = find_idx("R");
        let g_idx = find_idx("G");
        let b_idx = find_idx("B");
        let a_idx = find_idx("A");
        let z_idx = find_idx("Z");

        let exp_mult = 2.0_f32.powf(self.exposure);

        (0..pixel_count)
            .map(|pixel_idx| {
                let x = pixel_idx % w;
                let y = pixel_idx / w;
                let count = samples.sample_count(x, y);

                if count == 0 {
                    return Color32::BLACK;
                }

                let (rv, gv, bv) = match self.deep_mode {
                    DeepMode::SampleCount => {
                        // Heatmap: 0 = black, max = red
                        let max_count = 64.0; // Arbitrary max for heatmap
                        let t = (count as f32 / max_count).clamp(0.0, 1.0);
                        // Blue -> Cyan -> Green -> Yellow -> Red
                        heatmap_color(t)
                    }
                    DeepMode::Flattened => {
                        // Over composite all samples
                        self.composite_deep_pixel(samples, x, y, r_idx, g_idx, b_idx, a_idx)
                    }
                    DeepMode::FirstSample | DeepMode::LastSample => {
                        let sample_idx = if self.deep_mode == DeepMode::FirstSample {
                            0
                        } else {
                            count - 1
                        };
                        self.get_deep_sample(samples, x, y, sample_idx, r_idx, g_idx, b_idx)
                    }
                    DeepMode::MinDepth | DeepMode::MaxDepth => {
                        if let Some(z_i) = z_idx {
                            let depth = self.get_extreme_depth(
                                samples,
                                x,
                                y,
                                z_i,
                                self.deep_mode == DeepMode::MinDepth,
                            );
                            let d = self.normalize_depth(depth);
                            (d, d, d)
                        } else {
                            (0.0, 0.0, 0.0)
                        }
                    }
                    DeepMode::DepthSlice => {
                        // Composite only samples in slice range
                        self.composite_deep_slice(
                            samples, x, y, r_idx, g_idx, b_idx, a_idx, z_idx,
                        )
                    }
                };

                // Apply exposure (except for sample count mode)
                let (mut rv, mut gv, mut bv) = if self.deep_mode == DeepMode::SampleCount {
                    (rv, gv, bv)
                } else {
                    (rv * exp_mult, gv * exp_mult, bv * exp_mult)
                };

                // Apply sRGB
                if self.apply_srgb && self.deep_mode != DeepMode::SampleCount {
                    rv = linear_to_srgb(rv);
                    gv = linear_to_srgb(gv);
                    bv = linear_to_srgb(bv);
                }

                let rb = (rv.clamp(0.0, 1.0) * 255.0) as u8;
                let gb = (gv.clamp(0.0, 1.0) * 255.0) as u8;
                let bb = (bv.clamp(0.0, 1.0) * 255.0) as u8;

                Color32::from_rgb(rb, gb, bb)
            })
            .collect()
    }

    fn normalize_depth(&self, z: f32) -> f32 {
        let v = match self.depth_mode {
            DepthMode::Raw => z,
            DepthMode::AutoNormalize => {
                let (min, max) = (self.depth_near, self.depth_far);
                if max > min {
                    (z - min) / (max - min)
                } else {
                    z
                }
            }
            DepthMode::ManualRange => {
                if self.depth_far > self.depth_near {
                    (z - self.depth_near) / (self.depth_far - self.depth_near)
                } else {
                    z
                }
            }
            DepthMode::Logarithmic => {
                let near = self.depth_near.max(0.001);
                let far = self.depth_far.max(near + 0.001);
                let z = z.max(near);
                (z / near).ln() / (far / near).ln()
            }
        };

        if self.depth_invert {
            1.0 - v
        } else {
            v
        }
    }

    fn composite_deep_pixel(
        &self,
        samples: &crate::image::deep::DeepSamples,
        x: usize,
        y: usize,
        r_idx: Option<usize>,
        g_idx: Option<usize>,
        b_idx: Option<usize>,
        a_idx: Option<usize>,
    ) -> (f32, f32, f32) {
        let count = samples.sample_count(x, y);
        if count == 0 {
            return (0.0, 0.0, 0.0);
        }

        let w = samples.width;
        let pixel_idx = y * w + x;
        let (start, end) = samples.sample_range(pixel_idx);

        let mut accum_r = 0.0f32;
        let mut accum_g = 0.0f32;
        let mut accum_b = 0.0f32;
        let mut accum_a = 0.0f32;

        for i in start..end {
            let r = self.get_channel_sample(samples, r_idx, i).unwrap_or(0.0);
            let g = self.get_channel_sample(samples, g_idx, i).unwrap_or(0.0);
            let b = self.get_channel_sample(samples, b_idx, i).unwrap_or(0.0);
            let a = self.get_channel_sample(samples, a_idx, i).unwrap_or(1.0);

            // Over composite
            accum_r = r * a + accum_r * (1.0 - a);
            accum_g = g * a + accum_g * (1.0 - a);
            accum_b = b * a + accum_b * (1.0 - a);
            accum_a = a + accum_a * (1.0 - a);
        }

        (accum_r, accum_g, accum_b)
    }

    fn composite_deep_slice(
        &self,
        samples: &crate::image::deep::DeepSamples,
        x: usize,
        y: usize,
        r_idx: Option<usize>,
        g_idx: Option<usize>,
        b_idx: Option<usize>,
        a_idx: Option<usize>,
        z_idx: Option<usize>,
    ) -> (f32, f32, f32) {
        let count = samples.sample_count(x, y);
        if count == 0 {
            return (0.0, 0.0, 0.0);
        }

        let w = samples.width;
        let pixel_idx = y * w + x;
        let (start, end) = samples.sample_range(pixel_idx);

        let mut accum_r = 0.0f32;
        let mut accum_g = 0.0f32;
        let mut accum_b = 0.0f32;

        for i in start..end {
            // Check if in slice range
            if let Some(z_i) = z_idx {
                let z = self.get_channel_sample(samples, Some(z_i), i).unwrap_or(0.0);
                if z < self.slice_near || z > self.slice_far {
                    continue;
                }
            }

            let r = self.get_channel_sample(samples, r_idx, i).unwrap_or(0.0);
            let g = self.get_channel_sample(samples, g_idx, i).unwrap_or(0.0);
            let b = self.get_channel_sample(samples, b_idx, i).unwrap_or(0.0);
            let a = self.get_channel_sample(samples, a_idx, i).unwrap_or(1.0);

            accum_r = r * a + accum_r * (1.0 - a);
            accum_g = g * a + accum_g * (1.0 - a);
            accum_b = b * a + accum_b * (1.0 - a);
        }

        (accum_r, accum_g, accum_b)
    }

    fn get_deep_sample(
        &self,
        samples: &crate::image::deep::DeepSamples,
        x: usize,
        y: usize,
        sample_idx: usize,
        r_idx: Option<usize>,
        g_idx: Option<usize>,
        b_idx: Option<usize>,
    ) -> (f32, f32, f32) {
        let w = samples.width;
        let pixel_idx = y * w + x;
        let (start, _) = samples.sample_range(pixel_idx);
        let i = start + sample_idx;

        let r = self.get_channel_sample(samples, r_idx, i).unwrap_or(0.0);
        let g = self.get_channel_sample(samples, g_idx, i).unwrap_or(0.0);
        let b = self.get_channel_sample(samples, b_idx, i).unwrap_or(0.0);

        (r, g, b)
    }

    fn get_extreme_depth(
        &self,
        samples: &crate::image::deep::DeepSamples,
        x: usize,
        y: usize,
        z_idx: usize,
        is_min: bool,
    ) -> f32 {
        let count = samples.sample_count(x, y);
        if count == 0 {
            return 0.0;
        }

        let w = samples.width;
        let pixel_idx = y * w + x;
        let (start, end) = samples.sample_range(pixel_idx);

        let mut extreme = if is_min { f32::MAX } else { f32::MIN };

        for i in start..end {
            if let Some(z) = self.get_channel_sample(samples, Some(z_idx), i) {
                if is_min {
                    extreme = extreme.min(z);
                } else {
                    extreme = extreme.max(z);
                }
            }
        }

        extreme
    }

    fn get_channel_sample(
        &self,
        samples: &crate::image::deep::DeepSamples,
        ch_idx: Option<usize>,
        sample_idx: usize,
    ) -> Option<f32> {
        let ch_idx = ch_idx?;
        let ch = samples.channels.get(ch_idx)?;

        match ch {
            crate::image::deep::DeepChannelData::F32(data) => data.get(sample_idx).copied(),
            crate::image::deep::DeepChannelData::F16(data) => {
                data.get(sample_idx).map(|v| v.to_f32())
            }
            crate::image::deep::DeepChannelData::U32(data) => {
                data.get(sample_idx).map(|&v| v as f32)
            }
        }
    }

    fn zoom(&mut self, factor: f32) {
        self.zoom = (self.zoom * (1.0 + factor)).clamp(0.1, 100.0);
        self.send(ViewerEvent::StateSync {
            zoom: self.zoom,
            pan: self.pan,
        });
    }

    fn pan(&mut self, delta: [f32; 2]) {
        self.pan[0] += delta[0] / self.zoom;
        self.pan[1] += delta[1] / self.zoom;
        self.send(ViewerEvent::StateSync {
            zoom: self.zoom,
            pan: self.pan,
        });
    }

    fn fit_to_window(&mut self) {
        if let Some(image) = &self.image {
            let (img_w, img_h) = match image {
                LoadedImage::Flat(f) => f
                    .layer_data
                    .first()
                    .map(|l| (l.size.x() as f32, l.size.y() as f32))
                    .unwrap_or((1.0, 1.0)),
                LoadedImage::Deep(d) => {
                    (d.layer_data.size.x() as f32, d.layer_data.size.y() as f32)
                }
            };

            let vp_w = self.viewport[0];
            let vp_h = self.viewport[1];

            if img_w > 0.0 && img_h > 0.0 {
                self.zoom = (vp_w / img_w).min(vp_h / img_h) * 0.95;
                self.pan = [0.0, 0.0];
                self.send(ViewerEvent::StateSync {
                    zoom: self.zoom,
                    pan: self.pan,
                });
            }
        }
    }

    fn home(&mut self) {
        self.zoom = 1.0;
        self.pan = [0.0, 0.0];
        self.send(ViewerEvent::StateSync {
            zoom: self.zoom,
            pan: self.pan,
        });
    }
    
    /// Send depth data for 3D visualization.
    fn send_3d_data(&self) {
        let Some(image) = &self.image else {
            return;
        };
        
        // Extract depth channel data
        let (width, height, depth) = match image {
            LoadedImage::Flat(flat) => {
                let Some(layer) = flat.layer_data.first() else {
                    return;
                };
                let w = layer.size.x();
                let h = layer.size.y();
                
                // Try to find Z/depth channel, or use first channel
                let channel = layer.channel_data.list.iter()
                    .find(|c| c.name.eq_case_insensitive("z") 
                           || c.name.eq_case_insensitive("depth"))
                    .or_else(|| layer.channel_data.list.first());
                
                let Some(ch) = channel else {
                    return;
                };
                
                let depth: Vec<f32> = match &ch.sample_data {
                    FlatSamples::F32(data) => data.clone(),
                    FlatSamples::F16(data) => data.iter().map(|v| v.to_f32()).collect(),
                    FlatSamples::U32(data) => data.iter().map(|&v| v as f32).collect(),
                };
                
                (w, h, depth)
            }
            LoadedImage::Deep(deep) => {
                let w = deep.layer_data.size.x();
                let h = deep.layer_data.size.y();
                
                // Get first channel's DeepSamples (all channels share the same structure)
                let Some(first_ch) = deep.layer_data.channel_data.list.first() else {
                    return;
                };
                let samples = &first_ch.sample_data;
                
                // For deep data, use min depth per pixel
                // Find Z channel index in the deep samples
                let z_idx = samples.channels.iter()
                    .position(|c| matches!(c, crate::image::deep::DeepChannelData::F32(_)))
                    .unwrap_or(0);
                
                let mut depth = vec![0.0f32; w * h];
                
                for pixel_idx in 0..(w * h) {
                    let (start, end) = samples.sample_range(pixel_idx);
                    if start >= end {
                        continue;
                    }
                    
                    if let crate::image::deep::DeepChannelData::F32(z_data) = &samples.channels[z_idx] {
                        let min_z = z_data[start..end].iter()
                            .copied()
                            .filter(|z| z.is_finite())
                            .fold(f32::MAX, f32::min);
                        if min_z < f32::MAX {
                            depth[pixel_idx] = min_z;
                        }
                    }
                }
                
                (w, h, depth)
            }
        };
        
        self.send(ViewerEvent::Data3DReady { width, height, depth });
    }
}

/// Linear to sRGB gamma.
fn linear_to_srgb(x: f32) -> f32 {
    if x <= 0.0031308 {
        x * 12.92
    } else {
        1.055 * x.powf(1.0 / 2.4) - 0.055
    }
}

/// Heatmap: 0=blue, 0.25=cyan, 0.5=green, 0.75=yellow, 1=red
fn heatmap_color(t: f32) -> (f32, f32, f32) {
    let t = t.clamp(0.0, 1.0);
    if t < 0.25 {
        let s = t / 0.25;
        (0.0, s, 1.0)
    } else if t < 0.5 {
        let s = (t - 0.25) / 0.25;
        (0.0, 1.0, 1.0 - s)
    } else if t < 0.75 {
        let s = (t - 0.5) / 0.25;
        (s, 1.0, 0.0)
    } else {
        let s = (t - 0.75) / 0.25;
        (1.0, 1.0 - s, 0.0)
    }
}
