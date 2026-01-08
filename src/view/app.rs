//! Main viewer application with egui.

use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread::{self, JoinHandle};

use egui::{Color32, ColorImage, TextureHandle, TextureOptions, Vec2};

use crate::view::handler::ViewerHandler;
use crate::view::messages::{Generation, ViewerEvent, ViewerMsg};
use crate::view::state::{
    ChannelMode, DeepMode, DepthMode, DisplayMode, View3DMode, ViewerState,
};

/// Viewer configuration.
#[derive(Debug, Clone, Default)]
pub struct ViewerConfig {
    /// Verbosity level (0 = quiet).
    pub verbose: u8,
}

/// Main viewer application.
pub struct ViewerApp {
    tx: Sender<ViewerMsg>,
    rx: Receiver<ViewerEvent>,
    _worker: JoinHandle<()>,

    texture: Option<TextureHandle>,
    state: ViewerState,
    generation: Generation,
}

impl ViewerApp {
    /// Create new viewer app.
    pub fn new(
        _cc: &eframe::CreationContext<'_>,
        image_path: Option<PathBuf>,
        config: ViewerConfig,
    ) -> Self {
        let (tx_to_worker, rx_in_worker) = channel();
        let (tx_to_ui, rx_from_worker) = channel();

        let verbose = config.verbose;
        let worker = thread::spawn(move || {
            let handler = ViewerHandler::new(rx_in_worker, tx_to_ui, verbose);
            handler.run();
        });

        let app = Self {
            tx: tx_to_worker,
            rx: rx_from_worker,
            _worker: worker,
            texture: None,
            state: ViewerState::default(),
            generation: 0,
        };

        if let Some(path) = image_path {
            app.send(ViewerMsg::LoadImage(path));
        }

        app
    }

    fn send(&self, msg: ViewerMsg) {
        let _ = self.tx.send(msg);
    }

    fn send_regen(&mut self, msg: ViewerMsg) {
        self.generation += 1;
        self.send(ViewerMsg::SyncGeneration(self.generation));
        self.send(msg);
    }

    fn open_file_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("EXR", &["exr"])
            .add_filter("All", &["*"])
            .pick_file()
        {
            self.send(ViewerMsg::LoadImage(path));
        }
    }

    fn process_events(&mut self, ctx: &egui::Context) {
        while let Ok(event) = self.rx.try_recv() {
            match event {
                ViewerEvent::ImageLoaded {
                    path,
                    dims,
                    layers,
                    channels,
                    is_deep,
                    total_samples,
                    depth_range,
                } => {
                    self.state.image_path = Some(path.clone());
                    self.state.image_dims = Some(dims);
                    self.state.layers = layers.clone();
                    self.state.channels = channels.clone();
                    self.state.is_deep = is_deep;
                    self.state.total_samples = total_samples;
                    self.state.avg_samples = if dims.0 * dims.1 > 0 {
                        total_samples as f32 / (dims.0 * dims.1) as f32
                    } else {
                        0.0
                    };

                    if let Some(first) = layers.first() {
                        self.state.current_layer = first.clone();
                    }
                    if let Some(first) = channels.first() {
                        self.state.current_channel = first.clone();
                    }

                    if let Some((min, max)) = depth_range {
                        self.state.depth_auto_range = (min, max);
                        self.state.depth_near = min;
                        self.state.depth_far = max;
                        self.state.slice_near = min;
                        self.state.slice_far = max;
                    }

                    let title = format!(
                        "exrs view - {}",
                        path.file_name().and_then(|n| n.to_str()).unwrap_or("EXR")
                    );
                    ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
                    self.state.error = None;
                    
                    // Auto-fit on load
                    self.send(ViewerMsg::FitToWindow);
                }
                ViewerEvent::TextureReady {
                    generation,
                    width,
                    height,
                    pixels,
                } => {
                    if generation < self.generation {
                        continue;
                    }
                    let image = ColorImage {
                        size: [width, height],
                        pixels,
                    };
                    self.texture = Some(ctx.load_texture(
                        "exr_image",
                        image,
                        TextureOptions::LINEAR,
                    ));
                }
                ViewerEvent::StateSync { zoom, pan } => {
                    self.state.zoom = zoom;
                    self.state.pan = pan;
                }
                ViewerEvent::Error(msg) => {
                    self.state.error = Some(msg);
                }
            }
        }
    }

    fn handle_input(&mut self, ctx: &egui::Context) -> bool {
        let mut exit = false;

        ctx.input(|i| {
            if i.key_pressed(egui::Key::Escape) {
                exit = true;
            }
            if i.key_pressed(egui::Key::F) {
                self.send(ViewerMsg::FitToWindow);
            }
            if i.key_pressed(egui::Key::H) || i.key_pressed(egui::Key::Num0) {
                self.send(ViewerMsg::Home);
            }
            if i.key_pressed(egui::Key::Plus) || i.key_pressed(egui::Key::Equals) {
                self.send(ViewerMsg::Zoom { factor: 0.2 });
            }
            if i.key_pressed(egui::Key::Minus) {
                self.send(ViewerMsg::Zoom { factor: -0.2 });
            }

            // Channel shortcuts
            if i.key_pressed(egui::Key::R) && !i.modifiers.ctrl {
                self.state.channel_mode = ChannelMode::Red;
                self.send_regen(ViewerMsg::SetChannelMode(ChannelMode::Red));
            }
            if i.key_pressed(egui::Key::G) && !i.modifiers.ctrl {
                self.state.channel_mode = ChannelMode::Green;
                self.send_regen(ViewerMsg::SetChannelMode(ChannelMode::Green));
            }
            if i.key_pressed(egui::Key::B) && !i.modifiers.ctrl {
                self.state.channel_mode = ChannelMode::Blue;
                self.send_regen(ViewerMsg::SetChannelMode(ChannelMode::Blue));
            }
            if i.key_pressed(egui::Key::A) && !i.modifiers.ctrl {
                self.state.channel_mode = ChannelMode::Alpha;
                self.send_regen(ViewerMsg::SetChannelMode(ChannelMode::Alpha));
            }
            if i.key_pressed(egui::Key::C) && !i.modifiers.ctrl {
                self.state.channel_mode = ChannelMode::Color;
                self.send_regen(ViewerMsg::SetChannelMode(ChannelMode::Color));
            }
            if i.key_pressed(egui::Key::Z) && !i.modifiers.ctrl {
                self.state.channel_mode = ChannelMode::Depth;
                self.send_regen(ViewerMsg::SetChannelMode(ChannelMode::Depth));
            }
            if i.key_pressed(egui::Key::L) {
                self.state.channel_mode = ChannelMode::Luminance;
                self.send_regen(ViewerMsg::SetChannelMode(ChannelMode::Luminance));
            }

            // Scroll zoom
            if i.raw_scroll_delta.y != 0.0 {
                self.send(ViewerMsg::Zoom { factor: i.raw_scroll_delta.y * 0.002 });
            }

            // Ctrl+O open file
            if i.key_pressed(egui::Key::O) && i.modifiers.ctrl {
                self.open_file_dialog();
            }
        });

        exit
    }

    fn draw_controls(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("controls").show(ctx, |ui| {
            // Row 1: File, Mode, Layer, Channel
            ui.horizontal(|ui| {
                // Filename
                if let Some(ref path) = self.state.image_path {
                    let name = path.file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("?");
                    ui.strong(name);
                    ui.separator();
                }
                
                // 2D/3D toggle
                ui.selectable_value(&mut self.state.display_mode, DisplayMode::View2D, "2D");
                ui.selectable_value(&mut self.state.display_mode, DisplayMode::View3D, "3D");
                ui.separator();

                // Layer selector
                if self.state.layers.len() > 1 {
                    egui::ComboBox::from_label("Layer")
                        .selected_text(&self.state.current_layer)
                        .show_ui(ui, |ui| {
                            for layer in self.state.layers.clone() {
                                if ui
                                    .selectable_value(
                                        &mut self.state.current_layer,
                                        layer.clone(),
                                        &layer,
                                    )
                                    .changed()
                                {
                                    self.send_regen(ViewerMsg::SetLayer(layer));
                                }
                            }
                        });
                    ui.separator();
                }

                // Channel mode
                egui::ComboBox::from_label("Channel")
                    .selected_text(self.state.channel_mode.label())
                    .show_ui(ui, |ui| {
                        for &mode in ChannelMode::all_basic() {
                            let label = format!("{} ({})", mode.label(), mode.shortcut());
                            if ui
                                .selectable_value(&mut self.state.channel_mode, mode, label)
                                .changed()
                            {
                                self.send_regen(ViewerMsg::SetChannelMode(mode));
                            }
                        }
                        // Add custom channels
                        ui.separator();
                        let channels: Vec<_> = self.state.channels.clone();
                        for (i, ch) in channels.iter().enumerate() {
                            let mode = ChannelMode::Custom(i);
                            if ui
                                .selectable_value(&mut self.state.channel_mode, mode, ch)
                                .changed()
                            {
                                self.send_regen(ViewerMsg::SetChannel(ch.clone()));
                            }
                        }
                    });

                ui.separator();

                // Exposure
                ui.label("EV:");
                let old_exp = self.state.exposure;
                if ui
                    .add(
                        egui::Slider::new(&mut self.state.exposure, -10.0..=10.0)
                            .step_by(0.1)
                            .fixed_decimals(1),
                    )
                    .changed()
                    && (self.state.exposure - old_exp).abs() > 0.01
                {
                    self.send_regen(ViewerMsg::SetExposure(self.state.exposure));
                }

                // sRGB toggle
                if ui
                    .checkbox(&mut self.state.apply_srgb, "sRGB")
                    .changed()
                {
                    self.send_regen(ViewerMsg::SetSrgb(self.state.apply_srgb));
                }

                // Open file button (right side)
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Open...").clicked() {
                        self.open_file_dialog();
                    }
                    if ui.button("Refresh").clicked() {
                        self.send(ViewerMsg::Regenerate);
                    }
                });
            });

            // Row 2: Deep/Depth settings (if applicable)
            let show_deep = self.state.is_deep;
            let show_depth = matches!(self.state.channel_mode, ChannelMode::Depth);

            if show_deep || show_depth {
                ui.horizontal(|ui| {
                    if show_deep {
                        // Deep mode
                        egui::ComboBox::from_label("Deep")
                            .selected_text(self.state.deep_mode.label())
                            .show_ui(ui, |ui| {
                                for &mode in DeepMode::all() {
                                    if ui
                                        .selectable_value(
                                            &mut self.state.deep_mode,
                                            mode,
                                            mode.label(),
                                        )
                                        .changed()
                                    {
                                        self.send_regen(ViewerMsg::SetDeepMode(mode));
                                    }
                                }
                            });

                        // Slice controls for DepthSlice mode
                        if self.state.deep_mode == DeepMode::DepthSlice {
                            ui.separator();
                            ui.label("Slice:");
                            let range = self.state.depth_auto_range;
                            if ui
                                .add(
                                    egui::Slider::new(
                                        &mut self.state.slice_near,
                                        range.0..=range.1,
                                    )
                                    .text("Near"),
                                )
                                .changed()
                            {
                                self.send_regen(ViewerMsg::SetSliceRange(
                                    self.state.slice_near,
                                    self.state.slice_far,
                                ));
                            }
                            if ui
                                .add(
                                    egui::Slider::new(
                                        &mut self.state.slice_far,
                                        range.0..=range.1,
                                    )
                                    .text("Far"),
                                )
                                .changed()
                            {
                                self.send_regen(ViewerMsg::SetSliceRange(
                                    self.state.slice_near,
                                    self.state.slice_far,
                                ));
                            }
                        }

                        ui.separator();
                    }

                    if show_depth || show_deep {
                        // Depth normalization
                        egui::ComboBox::from_label("Normalize")
                            .selected_text(self.state.depth_mode.label())
                            .show_ui(ui, |ui| {
                                for &mode in DepthMode::all() {
                                    if ui
                                        .selectable_value(
                                            &mut self.state.depth_mode,
                                            mode,
                                            mode.label(),
                                        )
                                        .changed()
                                    {
                                        self.send_regen(ViewerMsg::SetDepthMode(mode));
                                    }
                                }
                            });

                        // Manual range
                        if self.state.depth_mode == DepthMode::ManualRange {
                            ui.label("Near:");
                            if ui
                                .add(egui::DragValue::new(&mut self.state.depth_near).speed(0.01))
                                .changed()
                            {
                                self.send_regen(ViewerMsg::SetDepthRange(
                                    self.state.depth_near,
                                    self.state.depth_far,
                                ));
                            }
                            ui.label("Far:");
                            if ui
                                .add(egui::DragValue::new(&mut self.state.depth_far).speed(0.01))
                                .changed()
                            {
                                self.send_regen(ViewerMsg::SetDepthRange(
                                    self.state.depth_near,
                                    self.state.depth_far,
                                ));
                            }
                        }

                        // Invert
                        if ui.checkbox(&mut self.state.depth_invert, "Invert").changed() {
                            self.send_regen(ViewerMsg::SetInvertDepth(self.state.depth_invert));
                        }
                    }
                });
            }

            // Row 3: 3D controls (if 3D mode)
            if self.state.display_mode == DisplayMode::View3D {
                ui.horizontal(|ui| {
                    egui::ComboBox::from_label("3D Mode")
                        .selected_text(self.state.view_3d_mode.label())
                        .show_ui(ui, |ui| {
                            for &mode in View3DMode::all() {
                                ui.selectable_value(
                                    &mut self.state.view_3d_mode,
                                    mode,
                                    mode.label(),
                                );
                            }
                        });

                    ui.separator();
                    ui.label("Point Size:");
                    ui.add(egui::Slider::new(&mut self.state.point_size, 1.0..=10.0));

                    ui.separator();
                    if ui.button("Reset Camera").clicked() {
                        self.state.camera_yaw = 0.0;
                        self.state.camera_pitch = 0.3;
                        self.state.camera_distance = 2.0;
                    }
                });
            }
        });
    }

    fn draw_status(&self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if self.state.image_dims.is_some() {
                    // Show image info when loaded
                    if let Some((w, h)) = self.state.image_dims {
                        ui.label(format!("{}x{}", w, h));
                        ui.separator();
                    }

                    ui.label(format!("{} ch", self.state.channels.len()));
                    ui.separator();

                    if self.state.is_deep {
                        ui.label(format!(
                            "Deep: {} ({:.1}/px)",
                            self.state.total_samples, self.state.avg_samples
                        ));
                        ui.separator();
                    }

                    let (min, max) = self.state.depth_auto_range;
                    if max > min {
                        ui.label(format!("Z: {:.2}..{:.2}", min, max));
                        ui.separator();
                    }

                    ui.label(format!("{}%", (self.state.zoom * 100.0) as i32));

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label("F:Fit H:1:1 +/-:Zoom R/G/B/A/Z:Ch");
                    });
                } else {
                    // No file loaded
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label("Ctrl+O: Open | Drag & drop EXR file");
                    });
                }
            });
        });
    }

    fn draw_canvas(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            let available = ui.available_size();

            // Track viewport size
            if (self.state.viewport_size[0] - available.x).abs() > 1.0
                || (self.state.viewport_size[1] - available.y).abs() > 1.0
            {
                self.state.viewport_size = [available.x, available.y];
                self.send(ViewerMsg::SetViewport(self.state.viewport_size));
            }

            // Error display
            if let Some(ref err) = self.state.error {
                ui.centered_and_justified(|ui| {
                    ui.colored_label(Color32::RED, err);
                });
                return;
            }

            match self.state.display_mode {
                DisplayMode::View2D => self.draw_2d_canvas(ui, available),
                DisplayMode::View3D => self.draw_3d_canvas(ui, available),
            }
        });
    }

    fn draw_2d_canvas(&mut self, ui: &mut egui::Ui, available: Vec2) {
        if let Some(ref texture) = self.texture {
            let tex_size = texture.size_vec2();
            let scaled_size = tex_size * self.state.zoom;

            let center = available / 2.0;
            let pan_offset = Vec2::new(
                self.state.pan[0] * self.state.zoom,
                self.state.pan[1] * self.state.zoom,
            );
            let top_left = center - scaled_size / 2.0 + pan_offset;

            let (rect, response) =
                ui.allocate_exact_size(available, egui::Sense::click_and_drag());

            if response.dragged() {
                let delta = response.drag_delta();
                self.send(ViewerMsg::Pan { delta: [delta.x, delta.y] });
            }
            if response.double_clicked() {
                self.send(ViewerMsg::FitToWindow);
            }

            let painter = ui.painter_at(rect);
            let image_rect =
                egui::Rect::from_min_size(rect.min + top_left.to_pos2().to_vec2(), scaled_size);
            painter.image(
                texture.id(),
                image_rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        } else {
            // Empty canvas - clickable area for file opening
            let (rect, response) = ui.allocate_exact_size(available, egui::Sense::click());
            
            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, 0.0, Color32::from_gray(24));
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "Double-click to open EXR\nor drag && drop file here",
                egui::FontId::proportional(16.0),
                Color32::from_gray(128),
            );
            
            if response.double_clicked() {
                self.open_file_dialog();
            }
        }
    }

    fn draw_3d_canvas(&mut self, ui: &mut egui::Ui, available: Vec2) {
        // 3D rendering placeholder
        // Will be implemented with three-d when view-3d feature is enabled
        let (rect, response) = ui.allocate_exact_size(available, egui::Sense::click_and_drag());

        // Camera orbit control
        if response.dragged() {
            let delta = response.drag_delta();
            self.state.camera_yaw += delta.x * 0.01;
            self.state.camera_pitch = (self.state.camera_pitch + delta.y * 0.01)
                .clamp(-1.5, 1.5);
        }

        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, Color32::from_gray(32));
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "3D View\n(requires view-3d feature)\n\nDrag to rotate camera",
            egui::FontId::default(),
            Color32::GRAY,
        );
    }

    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        ctx.input(|i| {
            if !i.raw.dropped_files.is_empty() {
                if let Some(path) = i.raw.dropped_files.first().and_then(|f| f.path.clone()) {
                    self.send(ViewerMsg::LoadImage(path));
                }
            }
        });
    }
}

impl eframe::App for ViewerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.process_events(ctx);
        self.handle_dropped_files(ctx);

        if self.handle_input(ctx) {
            self.send(ViewerMsg::Close);
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        self.draw_controls(ctx);
        self.draw_status(ctx);
        self.draw_canvas(ctx);

        ctx.request_repaint();
    }
}
