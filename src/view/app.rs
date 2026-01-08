//! Main viewer application with egui.

use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use egui::{Color32, ColorImage, TextureHandle, TextureOptions, Vec2};

use crate::view::handler::ViewerHandler;
use crate::view::messages::{Generation, ViewerEvent, ViewerMsg};
use crate::view::state::{
    ChannelMode, DeepMode, DepthMode, View3DMode, ViewerState,
};

#[cfg(feature = "view-3d")]
use crate::view::view3d::View3D;

#[cfg(feature = "view-3d")]
use egui_dock::{DockArea, DockState, NodeIndex, Style, TabViewer};

/// Dock panel tabs.
#[cfg(feature = "view-3d")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockTab {
    View2D,
    View3D,
}

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
    
    #[cfg(feature = "view-3d")]
    view3d: Option<Arc<Mutex<View3D>>>,
    
    #[cfg(feature = "view-3d")]
    dock_state: DockState<DockTab>,
}

impl std::fmt::Debug for ViewerApp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ViewerApp")
            .field("state", &self.state)
            .field("generation", &self.generation)
            .finish_non_exhaustive()
    }
}

impl ViewerApp {
    /// Create new viewer app.
    pub fn new(
        cc: &eframe::CreationContext<'_>,
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
        
        // Init 3D viewer with glow context
        #[cfg(feature = "view-3d")]
        let view3d = cc.gl.as_ref().map(|gl| Arc::new(Mutex::new(View3D::new(gl.clone()))));
        
        // Init dock state - just 2D view by default
        #[cfg(feature = "view-3d")]
        let dock_state = DockState::new(vec![DockTab::View2D]);

        let app = Self {
            tx: tx_to_worker,
            rx: rx_from_worker,
            _worker: worker,
            texture: None,
            state: ViewerState::default(),
            generation: 0,
            #[cfg(feature = "view-3d")]
            view3d,
            #[cfg(feature = "view-3d")]
            dock_state,
        };

        if let Some(path) = image_path {
            app.send(ViewerMsg::LoadImage(path));
        }

        app
    }

    fn send(&self, msg: ViewerMsg) {
        let _ = self.tx.send(msg);
    }
    
    /// Handle UI-local messages that don't need worker thread.
    #[cfg(feature = "view-3d")]
    fn handle_ui_msg(&mut self, msg: &ViewerMsg) {
        match msg {
            ViewerMsg::Reset3DCamera => {
                if let Some(view3d_arc) = &self.view3d {
                    if let Ok(mut view3d) = view3d_arc.lock() {
                        view3d.reset_camera();
                    }
                }
                // Reset local camera state too
                self.state.camera_yaw = 0.0;
                self.state.camera_pitch = 0.3;
                self.state.camera_distance = 2.0;
            }
            ViewerMsg::Toggle3D(enable) => {
                self.state.show_3d = *enable;
                if *enable {
                    self.send(ViewerMsg::Request3DData);
                }
            }
            ViewerMsg::SetPointSize(size) => {
                self.state.point_size = *size;
            }
            _ => {}
        }
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
                    let image = ColorImage::from_rgba_premultiplied(
                        [width, height],
                        &pixels.iter().flat_map(|c| c.to_array()).collect::<Vec<_>>(),
                    );
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
                #[cfg(feature = "view-3d")]
                ViewerEvent::Data3DReady { width, height, depth } => {
                    if let Some(view3d_arc) = &self.view3d {
                        if let Ok(mut view3d) = view3d_arc.lock() {
                            // Use appropriate method based on 3D mode
                            match self.state.view_3d_mode {
                                View3DMode::Heightfield => {
                                    view3d.set_heightfield(width, height, &depth);
                                }
                                View3DMode::PointCloud => {
                                    view3d.set_pointcloud(width, height, &depth);
                                }
                                View3DMode::PositionPass => {
                                    // TODO: need P.xyz channels, for now use depth as Y
                                    view3d.set_heightfield(width, height, &depth);
                                }
                            }
                        }
                    }
                }
                #[cfg(not(feature = "view-3d"))]
                ViewerEvent::Data3DReady { .. } => {}
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
                
                // 3D panel toggle
                let was_3d = self.state.show_3d;
                if ui.checkbox(&mut self.state.show_3d, "3D").changed() && self.state.show_3d != was_3d {
                    let msg = ViewerMsg::Toggle3D(self.state.show_3d);
                    #[cfg(feature = "view-3d")]
                    self.handle_ui_msg(&msg);
                    self.send(msg);
                }
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
                                        // Update 3D view if enabled
                                        if self.state.show_3d {
                                            self.send(ViewerMsg::Request3DData);
                                        }
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

            // Row 3: 3D controls (if 3D panel shown)
            if self.state.show_3d {
                ui.horizontal(|ui| {
                    let old_mode = self.state.view_3d_mode;
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
                    if self.state.view_3d_mode != old_mode {
                        self.send(ViewerMsg::Set3DMode(self.state.view_3d_mode));
                    }

                    ui.separator();
                    ui.label("Point Size:");
                    let old_size = self.state.point_size;
                    ui.add(egui::Slider::new(&mut self.state.point_size, 1.0..=10.0));
                    if (self.state.point_size - old_size).abs() > 0.01 {
                        let msg = ViewerMsg::SetPointSize(self.state.point_size);
                        self.handle_ui_msg(&msg);
                        self.send(msg);
                    }

                    ui.separator();
                    if ui.button("Reset Camera").clicked() {
                        let msg = ViewerMsg::Reset3DCamera;
                        self.handle_ui_msg(&msg);
                        self.send(msg);
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

    #[cfg(feature = "view-3d")]
    fn draw_canvas(&mut self, ctx: &egui::Context) {
        // Sync dock state with show_3d toggle
        self.sync_dock_with_3d_toggle();

        // Extract dock_state to avoid double borrow
        let mut dock_state = std::mem::replace(&mut self.dock_state, DockState::new(vec![DockTab::View2D]));
        
        // Render dock area
        DockArea::new(&mut dock_state)
            .style(Style::from_egui(ctx.style().as_ref()))
            .show_close_buttons(false)
            .show_tab_name_on_hover(false)
            .show(ctx, &mut DockTabs { app: self });
        
        // Put dock_state back
        self.dock_state = dock_state;
    }
    
    #[cfg(not(feature = "view-3d"))]
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

            self.draw_2d_canvas(ui, available);
        });
    }
    
    /// Sync dock layout when 3D toggle changes.
    #[cfg(feature = "view-3d")]
    fn sync_dock_with_3d_toggle(&mut self) {
        // Check if 3D tab exists
        let has_3d = self.dock_state
            .iter_all_tabs()
            .any(|(_, tab)| *tab == DockTab::View3D);
        
        if self.state.show_3d && !has_3d {
            // Rebuild dock with both panels: 2D left (0.6), 3D right (0.4)
            let mut dock = DockState::new(vec![DockTab::View2D]);
            dock.main_surface_mut().split_right(NodeIndex::root(), 0.4, vec![DockTab::View3D]);
            self.dock_state = dock;
        } else if !self.state.show_3d && has_3d {
            // Remove 3D tab - rebuild dock with just 2D
            self.dock_state = DockState::new(vec![DockTab::View2D]);
        }
    }

    fn draw_2d_canvas(&mut self, ui: &mut egui::Ui, available: Vec2) {
        // Track viewport size for fit-to-window calculation
        if (self.state.viewport_size[0] - available.x).abs() > 1.0
            || (self.state.viewport_size[1] - available.y).abs() > 1.0
        {
            self.state.viewport_size = [available.x, available.y];
            self.send(ViewerMsg::SetViewport(self.state.viewport_size));
        }
        
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
            
            // Scroll zoom only when hovered over 2D canvas
            if response.hovered() {
                let scroll = ui.input(|i| i.raw_scroll_delta.y);
                if scroll != 0.0 {
                    self.send(ViewerMsg::Zoom { factor: scroll * 0.002 });
                }
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

    #[cfg(feature = "view-3d")]
    fn draw_3d_canvas(&mut self, ui: &mut egui::Ui, available: Vec2) {
        use three_d::{Event, MouseButton, PhysicalPoint};
        
        let (rect, response) = ui.allocate_exact_size(available, egui::Sense::click_and_drag());
        
        // Check if we have 3D view initialized
        let Some(view3d_arc) = self.view3d.clone() else {
            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, 0.0, Color32::from_gray(32));
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "3D View: No OpenGL context\n\nEnsure glow backend is enabled",
                egui::FontId::default(),
                Color32::GRAY,
            );
            return;
        };
        
        // Check if 3D data is loaded
        let has_data = if let Ok(view3d) = view3d_arc.lock() {
            view3d.has_data()
        } else {
            false
        };
        
        if !has_data {
            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, 0.0, Color32::from_gray(32));
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "No depth data\n\nSwitch to Z channel or load\nan image with depth",
                egui::FontId::default(),
                Color32::from_gray(120),
            );
            return;
        }
        
        // Double-click to open file dialog (same as 2D)
        if response.double_clicked() {
            self.open_file_dialog();
        }
        
        // Convert egui input to three-d events
        let mut events = Vec::new();
        
        // Get mouse button state and delta
        let dominated = response.dragged();
        let delta = response.drag_delta();
        
        if dominated && (delta.x.abs() > 0.1 || delta.y.abs() > 0.1) {
            // Check which button is pressed via input state
            let (left, middle, right) = ui.input(|i| {
                (
                    i.pointer.button_down(egui::PointerButton::Primary),
                    i.pointer.button_down(egui::PointerButton::Middle),
                    i.pointer.button_down(egui::PointerButton::Secondary),
                )
            });
            
            let button = if middle {
                Some(MouseButton::Middle)
            } else if right {
                Some(MouseButton::Right)
            } else if left {
                Some(MouseButton::Left)
            } else {
                None
            };
            
            if button.is_some() {
                events.push(Event::MouseMotion {
                    button,
                    delta: (delta.x, delta.y),
                    position: PhysicalPoint { x: 0.0, y: 0.0 },
                    modifiers: Default::default(),
                    handled: false,
                });
            }
        }
        
        // Scroll for zoom - only when hovered over 3D canvas
        if response.hovered() {
            let scroll = ui.input(|i| i.raw_scroll_delta.y);
            if scroll.abs() > 0.1 {
                events.push(Event::MouseWheel {
                    delta: (0.0, scroll * 0.1),
                    position: PhysicalPoint { x: 0.0, y: 0.0 },
                    modifiers: Default::default(),
                    handled: false,
                });
            }
        }
        
        // Handle events
        if let Ok(mut view3d) = view3d_arc.lock() {
            view3d.handle_events(&mut events);
            
            // Keyboard shortcuts
            ui.input(|i| {
                if i.key_pressed(egui::Key::G) {
                    view3d.toggle_grid();
                }
                if i.key_pressed(egui::Key::W) {
                    view3d.toggle_wireframe();
                }
                if i.key_pressed(egui::Key::Home) {
                    view3d.reset_camera();
                }
            });
        }
        
        // Use paint callback to render three-d
        let view3d_for_callback = view3d_arc.clone();
        let callback = egui::PaintCallback {
            rect,
            callback: std::sync::Arc::new(eframe::egui_glow::CallbackFn::new(move |info, _painter| {
                let vp = info.clip_rect_in_pixels();
                
                // OpenGL viewport needs absolute window coordinates
                let viewport = three_d::Viewport {
                    x: vp.left_px,
                    y: vp.from_bottom_px,
                    width: vp.width_px as u32,
                    height: vp.height_px as u32,
                };
                
                if let Ok(view3d) = view3d_for_callback.lock() {
                    view3d.render(viewport);
                }
            })),
        };
        
        ui.painter().add(callback);
    }

    #[cfg(not(feature = "view-3d"))]
    fn draw_3d_canvas(&mut self, ui: &mut egui::Ui, available: Vec2) {
        let (rect, _response) = ui.allocate_exact_size(available, egui::Sense::hover());
        
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, Color32::from_gray(32));
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "3D View disabled\n\nRebuild with: cargo build --features view-3d",
            egui::FontId::default(),
            Color32::from_gray(100),
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

// === DockTabs wrapper for egui_dock ===

#[cfg(feature = "view-3d")]
pub struct DockTabs<'a> {
    pub app: &'a mut ViewerApp,
}

#[cfg(feature = "view-3d")]
impl<'a> TabViewer for DockTabs<'a> {
    type Tab = DockTab;

    fn title(&mut self, tab: &mut DockTab) -> egui::WidgetText {
        match tab {
            DockTab::View2D => "2D".into(),
            DockTab::View3D => "3D".into(),
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut DockTab) {
        let available = ui.available_size();
        match tab {
            DockTab::View2D => self.app.draw_2d_canvas(ui, available),
            DockTab::View3D => self.app.draw_3d_canvas(ui, available),
        }
    }
}
