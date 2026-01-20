use eframe::egui;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 測定状態のステートマシン
#[derive(Default)]
enum MeasurementState {
    #[default]
    Idle,
    FirstPointSelected(egui::Pos2),
}

/// キャリブレーション状態
#[derive(Default)]
enum CalibrationState {
    #[default]
    Idle,
    FirstPointSelected(egui::Pos2),
    WaitingForInput {
        start: egui::Pos2,
        end: egui::Pos2,
        distance_px: f32,
    },
}

/// 測定モード
#[derive(Default, PartialEq, Clone, Copy)]
enum MeasurementMode {
    #[default]
    Line,
    Rectangle,
}

/// 測定結果
#[derive(Clone, Serialize, Deserialize)]
struct Measurement {
    start: (f32, f32),
    end: (f32, f32),
    distance_px: f32,
}

impl Measurement {
    fn new(start: egui::Pos2, end: egui::Pos2) -> Self {
        let distance_px = start.distance(end);
        Self {
            start: (start.x, start.y),
            end: (end.x, end.y),
            distance_px,
        }
    }

    fn start_pos(&self) -> egui::Pos2 {
        egui::pos2(self.start.0, self.start.1)
    }

    fn end_pos(&self) -> egui::Pos2 {
        egui::pos2(self.end.0, self.end.1)
    }

    fn distance_with_calibration(&self, calibration: Option<&Calibration>) -> (f32, String) {
        match calibration {
            Some(cal) => (self.distance_px / cal.pixels_per_unit, cal.unit_name.clone()),
            None => (self.distance_px, "px".to_string()),
        }
    }
}

/// 矩形測定結果
#[derive(Clone, Serialize, Deserialize)]
struct RectangleMeasurement {
    corner1: (f32, f32),
    corner2: (f32, f32),
    width_px: f32,
    height_px: f32,
    area_px: f32,
}

impl RectangleMeasurement {
    fn new(corner1: egui::Pos2, corner2: egui::Pos2) -> Self {
        let width_px = (corner2.x - corner1.x).abs();
        let height_px = (corner2.y - corner1.y).abs();
        Self {
            corner1: (corner1.x, corner1.y),
            corner2: (corner2.x, corner2.y),
            width_px,
            height_px,
            area_px: width_px * height_px,
        }
    }

    fn min_corner(&self) -> egui::Pos2 {
        egui::pos2(
            self.corner1.0.min(self.corner2.0),
            self.corner1.1.min(self.corner2.1),
        )
    }

    fn max_corner(&self) -> egui::Pos2 {
        egui::pos2(
            self.corner1.0.max(self.corner2.0),
            self.corner1.1.max(self.corner2.1),
        )
    }

    fn dimensions_with_calibration(
        &self,
        calibration: Option<&Calibration>,
    ) -> (f32, f32, f32, String) {
        match calibration {
            Some(cal) => {
                let width = self.width_px / cal.pixels_per_unit;
                let height = self.height_px / cal.pixels_per_unit;
                let area = self.area_px / (cal.pixels_per_unit * cal.pixels_per_unit);
                (width, height, area, cal.unit_name.clone())
            }
            None => (self.width_px, self.height_px, self.area_px, "px".to_string()),
        }
    }
}

/// キャリブレーション設定
#[derive(Clone, Serialize, Deserialize)]
struct Calibration {
    pixels_per_unit: f32,
    unit_name: String,
}

/// エクスポート用のデータ構造
#[derive(Serialize)]
struct ExportData {
    calibration: Option<Calibration>,
    measurements: Vec<ExportMeasurement>,
    rectangle_measurements: Vec<ExportRectangleMeasurement>,
}

#[derive(Serialize)]
struct ExportMeasurement {
    id: usize,
    start_x: f32,
    start_y: f32,
    end_x: f32,
    end_y: f32,
    distance_px: f32,
    distance_calibrated: Option<f32>,
    unit: String,
}

#[derive(Serialize)]
struct ExportRectangleMeasurement {
    id: usize,
    corner1_x: f32,
    corner1_y: f32,
    corner2_x: f32,
    corner2_y: f32,
    width_px: f32,
    height_px: f32,
    area_px: f32,
    width_calibrated: Option<f32>,
    height_calibrated: Option<f32>,
    area_calibrated: Option<f32>,
    unit: String,
}

/// アプリケーション状態
struct SampoApp {
    image_texture: Option<egui::TextureHandle>,
    image_dimensions: Option<(u32, u32)>,
    image_path: Option<String>,
    measurement_state: MeasurementState,
    measurement_mode: MeasurementMode,
    measurements: Vec<Measurement>,
    rectangle_measurements: Vec<RectangleMeasurement>,
    calibration: Option<Calibration>,
    calibration_state: CalibrationState,
    calibration_input: String,
    calibration_unit: String,
    zoom: f32,
    is_calibrating: bool,
    text_color: egui::Color32,
    scroll_offset: egui::Vec2,
    needs_scroll_reset: bool,
    show_preview: bool,
    current_mouse_image_pos: Option<egui::Pos2>,
}

impl Default for SampoApp {
    fn default() -> Self {
        Self {
            image_texture: None,
            image_dimensions: None,
            image_path: None,
            measurement_state: MeasurementState::default(),
            measurement_mode: MeasurementMode::default(),
            measurements: Vec::new(),
            rectangle_measurements: Vec::new(),
            calibration: None,
            calibration_state: CalibrationState::default(),
            calibration_input: String::new(),
            calibration_unit: "mm".to_string(),
            zoom: 1.0,
            is_calibrating: false,
            text_color: egui::Color32::WHITE,
            scroll_offset: egui::Vec2::ZERO,
            needs_scroll_reset: false,
            show_preview: true,
            current_mouse_image_pos: None,
        }
    }
}

impl SampoApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        egui_extras::install_image_loaders(&cc.egui_ctx);

        // NotoSans JP フォントを設定
        let mut fonts = egui::FontDefinitions::default();

        fonts.font_data.insert(
            "NotoSansJP".to_owned(),
            std::sync::Arc::new(egui::FontData::from_static(include_bytes!(
                "../assets/NotoSansJP-Regular.ttf"
            ))),
        );

        // 日本語フォントを優先的に使用
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "NotoSansJP".to_owned());

        fonts
            .families
            .entry(egui::FontFamily::Monospace)
            .or_default()
            .insert(0, "NotoSansJP".to_owned());

        cc.egui_ctx.set_fonts(fonts);

        Self::default()
    }

    fn open_file_dialog(&mut self, ctx: &egui::Context) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Images", &["png", "jpg", "jpeg", "gif", "bmp", "webp"])
            .pick_file()
        {
            self.load_image(ctx, &path);
        }
    }

    fn load_image(&mut self, ctx: &egui::Context, path: &PathBuf) {
        match image::open(path) {
            Ok(img) => {
                let rgba = img.to_rgba8();
                let dimensions = rgba.dimensions();

                let color_image = egui::ColorImage::from_rgba_unmultiplied(
                    [dimensions.0 as usize, dimensions.1 as usize],
                    rgba.as_raw(),
                );

                let texture = ctx.load_texture(
                    path.to_string_lossy(),
                    color_image,
                    egui::TextureOptions::LINEAR,
                );

                self.image_texture = Some(texture);
                self.image_dimensions = Some(dimensions);
                self.image_path = Some(path.to_string_lossy().into_owned());
                self.measurements.clear();
                self.rectangle_measurements.clear();
                self.measurement_state = MeasurementState::Idle;
                self.calibration = None;
                self.calibration_state = CalibrationState::Idle;
                self.is_calibrating = false;
                self.zoom = 1.0;
                self.needs_scroll_reset = true;
            }
            Err(e) => {
                eprintln!("Failed to load image: {}", e);
            }
        }
    }

    fn screen_to_image(&self, screen_pos: egui::Pos2, image_rect: egui::Rect) -> egui::Pos2 {
        if let Some((w, h)) = self.image_dimensions {
            let normalized = (screen_pos - image_rect.min) / image_rect.size();
            egui::pos2(normalized.x * w as f32, normalized.y * h as f32)
        } else {
            screen_pos
        }
    }

    fn image_to_screen(&self, image_pos: egui::Pos2, image_rect: egui::Rect) -> egui::Pos2 {
        if let Some((w, h)) = self.image_dimensions {
            let normalized = egui::vec2(image_pos.x / w as f32, image_pos.y / h as f32);
            image_rect.min + normalized * image_rect.size()
        } else {
            image_pos
        }
    }

    fn handle_canvas_click(&mut self, click_pos: egui::Pos2, image_rect: egui::Rect) {
        let image_pos = self.screen_to_image(click_pos, image_rect);

        if self.is_calibrating {
            match &self.calibration_state {
                CalibrationState::Idle => {
                    self.calibration_state = CalibrationState::FirstPointSelected(image_pos);
                }
                CalibrationState::FirstPointSelected(start) => {
                    let start = *start;
                    let distance_px = start.distance(image_pos);
                    self.calibration_state = CalibrationState::WaitingForInput {
                        start,
                        end: image_pos,
                        distance_px,
                    };
                }
                CalibrationState::WaitingForInput { .. } => {}
            }
        } else {
            match &self.measurement_state {
                MeasurementState::Idle => {
                    self.measurement_state = MeasurementState::FirstPointSelected(image_pos);
                }
                MeasurementState::FirstPointSelected(start) => {
                    match self.measurement_mode {
                        MeasurementMode::Line => {
                            let measurement = Measurement::new(*start, image_pos);
                            self.measurements.push(measurement);
                        }
                        MeasurementMode::Rectangle => {
                            let rect_measurement = RectangleMeasurement::new(*start, image_pos);
                            self.rectangle_measurements.push(rect_measurement);
                        }
                    }
                    self.measurement_state = MeasurementState::Idle;
                }
            }
        }
    }

    fn draw_measurements(&self, painter: &egui::Painter, image_rect: egui::Rect) {
        let line_color = egui::Color32::from_rgb(255, 100, 100);
        let point_color = egui::Color32::from_rgb(100, 255, 100);
        let stroke = egui::Stroke::new(2.0, line_color);
        let point_radius = 5.0;

        for measurement in &self.measurements {
            let start_screen = self.image_to_screen(measurement.start_pos(), image_rect);
            let end_screen = self.image_to_screen(measurement.end_pos(), image_rect);

            painter.line_segment([start_screen, end_screen], stroke);
            painter.circle_filled(start_screen, point_radius, point_color);
            painter.circle_filled(end_screen, point_radius, point_color);

            let midpoint = start_screen + (end_screen - start_screen) * 0.5;
            let (distance, unit) = measurement.distance_with_calibration(self.calibration.as_ref());
            painter.text(
                midpoint + egui::vec2(0.0, -15.0),
                egui::Align2::CENTER_BOTTOM,
                format!("{:.1} {}", distance, unit),
                egui::FontId::default(),
                self.text_color,
            );
        }

        // 矩形測定を描画
        let rect_color = egui::Color32::from_rgb(100, 150, 255);
        let rect_stroke = egui::Stroke::new(2.0, rect_color);

        for rect_m in &self.rectangle_measurements {
            let min_screen = self.image_to_screen(rect_m.min_corner(), image_rect);
            let max_screen = self.image_to_screen(rect_m.max_corner(), image_rect);

            // 4辺を描画
            let top_left = min_screen;
            let top_right = egui::pos2(max_screen.x, min_screen.y);
            let bottom_left = egui::pos2(min_screen.x, max_screen.y);
            let bottom_right = max_screen;

            painter.line_segment([top_left, top_right], rect_stroke);
            painter.line_segment([top_right, bottom_right], rect_stroke);
            painter.line_segment([bottom_right, bottom_left], rect_stroke);
            painter.line_segment([bottom_left, top_left], rect_stroke);

            // 4つの角に点を描画
            painter.circle_filled(top_left, point_radius, point_color);
            painter.circle_filled(top_right, point_radius, point_color);
            painter.circle_filled(bottom_left, point_radius, point_color);
            painter.circle_filled(bottom_right, point_radius, point_color);

            let (width, height, area, unit) =
                rect_m.dimensions_with_calibration(self.calibration.as_ref());

            // 幅ラベル（上辺の中央）
            let width_pos = egui::pos2((top_left.x + top_right.x) / 2.0, top_left.y - 15.0);
            painter.text(
                width_pos,
                egui::Align2::CENTER_BOTTOM,
                format!("{:.1} {}", width, unit),
                egui::FontId::default(),
                self.text_color,
            );

            // 高さラベル（左辺の中央）
            let height_pos = egui::pos2(top_left.x - 10.0, (top_left.y + bottom_left.y) / 2.0);
            painter.text(
                height_pos,
                egui::Align2::RIGHT_CENTER,
                format!("{:.1} {}", height, unit),
                egui::FontId::default(),
                self.text_color,
            );

            // 面積ラベル（中央）
            let area_unit = if unit == "px" {
                "px²".to_string()
            } else {
                format!("{}²", unit)
            };
            let center = egui::pos2(
                (top_left.x + bottom_right.x) / 2.0,
                (top_left.y + bottom_right.y) / 2.0,
            );
            painter.text(
                center,
                egui::Align2::CENTER_CENTER,
                format!("{:.1} {}", area, area_unit),
                egui::FontId::default(),
                self.text_color,
            );
        }

        // 測定中の線を描画
        if let MeasurementState::FirstPointSelected(start) = &self.measurement_state {
            let start_screen = self.image_to_screen(*start, image_rect);
            painter.circle_filled(start_screen, point_radius, egui::Color32::YELLOW);

            // プレビュー描画
            if self.show_preview {
                if let Some(mouse_pos) = self.current_mouse_image_pos {
                    let mouse_screen = self.image_to_screen(mouse_pos, image_rect);
                    let preview_color = egui::Color32::from_rgba_unmultiplied(255, 255, 0, 150);
                    let preview_stroke = egui::Stroke::new(1.5, preview_color);

                    match self.measurement_mode {
                        MeasurementMode::Line => {
                            // 線分のプレビュー
                            painter.line_segment([start_screen, mouse_screen], preview_stroke);
                            painter.circle_filled(
                                mouse_screen,
                                point_radius * 0.7,
                                preview_color,
                            );

                            // 距離のプレビュー表示
                            let distance_px = start.distance(mouse_pos);
                            let (distance, unit) = match &self.calibration {
                                Some(cal) => {
                                    (distance_px / cal.pixels_per_unit, cal.unit_name.clone())
                                }
                                None => (distance_px, "px".to_string()),
                            };
                            let midpoint = start_screen + (mouse_screen - start_screen) * 0.5;
                            painter.text(
                                midpoint + egui::vec2(0.0, -15.0),
                                egui::Align2::CENTER_BOTTOM,
                                format!("{:.1} {}", distance, unit),
                                egui::FontId::default(),
                                self.text_color,
                            );
                        }
                        MeasurementMode::Rectangle => {
                            // 矩形のプレビュー
                            let min_x = start_screen.x.min(mouse_screen.x);
                            let max_x = start_screen.x.max(mouse_screen.x);
                            let min_y = start_screen.y.min(mouse_screen.y);
                            let max_y = start_screen.y.max(mouse_screen.y);

                            let top_left = egui::pos2(min_x, min_y);
                            let top_right = egui::pos2(max_x, min_y);
                            let bottom_left = egui::pos2(min_x, max_y);
                            let bottom_right = egui::pos2(max_x, max_y);

                            painter.line_segment([top_left, top_right], preview_stroke);
                            painter.line_segment([top_right, bottom_right], preview_stroke);
                            painter.line_segment([bottom_right, bottom_left], preview_stroke);
                            painter.line_segment([bottom_left, top_left], preview_stroke);

                            painter.circle_filled(top_left, point_radius * 0.7, preview_color);
                            painter.circle_filled(top_right, point_radius * 0.7, preview_color);
                            painter.circle_filled(bottom_left, point_radius * 0.7, preview_color);
                            painter.circle_filled(bottom_right, point_radius * 0.7, preview_color);

                            // 寸法のプレビュー表示
                            let width_px = (mouse_pos.x - start.x).abs();
                            let height_px = (mouse_pos.y - start.y).abs();
                            let area_px = width_px * height_px;

                            let (width, height, area, unit) = match &self.calibration {
                                Some(cal) => {
                                    let w = width_px / cal.pixels_per_unit;
                                    let h = height_px / cal.pixels_per_unit;
                                    let a =
                                        area_px / (cal.pixels_per_unit * cal.pixels_per_unit);
                                    (w, h, a, cal.unit_name.clone())
                                }
                                None => (width_px, height_px, area_px, "px".to_string()),
                            };

                            // 幅ラベル
                            let width_pos = egui::pos2((top_left.x + top_right.x) / 2.0, min_y - 15.0);
                            painter.text(
                                width_pos,
                                egui::Align2::CENTER_BOTTOM,
                                format!("{:.1} {}", width, unit),
                                egui::FontId::default(),
                                self.text_color,
                            );

                            // 高さラベル
                            let height_pos = egui::pos2(min_x - 10.0, (min_y + max_y) / 2.0);
                            painter.text(
                                height_pos,
                                egui::Align2::RIGHT_CENTER,
                                format!("{:.1} {}", height, unit),
                                egui::FontId::default(),
                                self.text_color,
                            );

                            // 面積ラベル
                            let area_unit = if unit == "px" {
                                "px²".to_string()
                            } else {
                                format!("{}²", unit)
                            };
                            let center = egui::pos2((min_x + max_x) / 2.0, (min_y + max_y) / 2.0);
                            painter.text(
                                center,
                                egui::Align2::CENTER_CENTER,
                                format!("{:.1} {}", area, area_unit),
                                egui::FontId::default(),
                                self.text_color,
                            );
                        }
                    }
                }
            }
        }

        // キャリブレーション中の線を描画
        match &self.calibration_state {
            CalibrationState::FirstPointSelected(start) => {
                let start_screen = self.image_to_screen(*start, image_rect);
                painter.circle_filled(start_screen, point_radius, egui::Color32::LIGHT_BLUE);
            }
            CalibrationState::WaitingForInput { start, end, .. } => {
                let start_screen = self.image_to_screen(*start, image_rect);
                let end_screen = self.image_to_screen(*end, image_rect);
                let calib_stroke = egui::Stroke::new(2.0, egui::Color32::LIGHT_BLUE);
                painter.line_segment([start_screen, end_screen], calib_stroke);
                painter.circle_filled(start_screen, point_radius, egui::Color32::LIGHT_BLUE);
                painter.circle_filled(end_screen, point_radius, egui::Color32::LIGHT_BLUE);
            }
            _ => {}
        }
    }

    fn show_image_canvas(&mut self, ui: &mut egui::Ui, viewport_size: egui::Vec2) {
        let Some(texture) = &self.image_texture else {
            ui.centered_and_justified(|ui| {
                ui.label("画像が読み込まれていません。「画像を開く」をクリックしてください。");
            });
            return;
        };

        let texture_size = texture.size_vec2() * self.zoom;
        let texture_id = texture.id();

        // 画像の外側にもスクロールできるようにパディングを追加
        let padding = viewport_size;

        // 上部パディング
        ui.allocate_space(egui::vec2(texture_size.x + padding.x * 2.0, padding.y));

        // 画像描画用の情報を保持
        let mut image_rect = None;
        let mut clicked_pos = None;
        let mut hover_pos = None;

        ui.horizontal(|ui| {
            // 左パディング
            ui.allocate_space(egui::vec2(padding.x, texture_size.y));

            // 画像を描画
            let (response, painter) =
                ui.allocate_painter(texture_size, egui::Sense::click_and_drag());

            painter.image(
                texture_id,
                response.rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );

            image_rect = Some(response.rect);

            if response.clicked() {
                clicked_pos = response.interact_pointer_pos();
            }

            // ホバー位置を取得
            hover_pos = response.hover_pos();

            // 右パディング
            ui.allocate_space(egui::vec2(padding.x, texture_size.y));
        });

        // 下部パディング
        ui.allocate_space(egui::vec2(texture_size.x + padding.x * 2.0, padding.y));

        // クリック処理と測定線描画
        if let Some(rect) = image_rect {
            // マウス位置を画像座標に変換して保存
            self.current_mouse_image_pos = hover_pos.map(|pos| self.screen_to_image(pos, rect));

            if let Some(pointer_pos) = clicked_pos {
                self.handle_canvas_click(pointer_pos, rect);
            }

            // 測定線を描画（別のPainterを使用）
            let painter = ui.painter_at(rect);
            self.draw_measurements(&painter, rect);
        } else {
            self.current_mouse_image_pos = None;
        }
    }

    fn export_csv(&self) -> String {
        let mut csv = String::new();

        // 線分測定
        if !self.measurements.is_empty() {
            csv.push_str("# Line Measurements\n");
            csv.push_str("id,start_x,start_y,end_x,end_y,distance_px,distance_calibrated,unit\n");
            for (i, m) in self.measurements.iter().enumerate() {
                let (distance, unit) = m.distance_with_calibration(self.calibration.as_ref());
                let calibrated = if self.calibration.is_some() {
                    format!("{:.2}", distance)
                } else {
                    String::new()
                };
                csv.push_str(&format!(
                    "{},{:.2},{:.2},{:.2},{:.2},{:.2},{},{}\n",
                    i + 1,
                    m.start.0,
                    m.start.1,
                    m.end.0,
                    m.end.1,
                    m.distance_px,
                    calibrated,
                    unit
                ));
            }
        }

        // 矩形測定
        if !self.rectangle_measurements.is_empty() {
            if !csv.is_empty() {
                csv.push('\n');
            }
            csv.push_str("# Rectangle Measurements\n");
            csv.push_str("id,corner1_x,corner1_y,corner2_x,corner2_y,width_px,height_px,area_px,width_calibrated,height_calibrated,area_calibrated,unit\n");
            for (i, rm) in self.rectangle_measurements.iter().enumerate() {
                let (width, height, area, unit) =
                    rm.dimensions_with_calibration(self.calibration.as_ref());
                let (w_cal, h_cal, a_cal) = if self.calibration.is_some() {
                    (
                        format!("{:.2}", width),
                        format!("{:.2}", height),
                        format!("{:.2}", area),
                    )
                } else {
                    (String::new(), String::new(), String::new())
                };
                csv.push_str(&format!(
                    "{},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{},{},{},{}\n",
                    i + 1,
                    rm.corner1.0,
                    rm.corner1.1,
                    rm.corner2.0,
                    rm.corner2.1,
                    rm.width_px,
                    rm.height_px,
                    rm.area_px,
                    w_cal,
                    h_cal,
                    a_cal,
                    unit
                ));
            }
        }

        csv
    }

    fn export_json(&self) -> String {
        let measurements: Vec<ExportMeasurement> = self
            .measurements
            .iter()
            .enumerate()
            .map(|(i, m)| {
                let (distance, unit) = m.distance_with_calibration(self.calibration.as_ref());
                ExportMeasurement {
                    id: i + 1,
                    start_x: m.start.0,
                    start_y: m.start.1,
                    end_x: m.end.0,
                    end_y: m.end.1,
                    distance_px: m.distance_px,
                    distance_calibrated: if self.calibration.is_some() {
                        Some(distance)
                    } else {
                        None
                    },
                    unit,
                }
            })
            .collect();

        let rectangle_measurements: Vec<ExportRectangleMeasurement> = self
            .rectangle_measurements
            .iter()
            .enumerate()
            .map(|(i, rm)| {
                let (width, height, area, unit) =
                    rm.dimensions_with_calibration(self.calibration.as_ref());
                ExportRectangleMeasurement {
                    id: i + 1,
                    corner1_x: rm.corner1.0,
                    corner1_y: rm.corner1.1,
                    corner2_x: rm.corner2.0,
                    corner2_y: rm.corner2.1,
                    width_px: rm.width_px,
                    height_px: rm.height_px,
                    area_px: rm.area_px,
                    width_calibrated: if self.calibration.is_some() {
                        Some(width)
                    } else {
                        None
                    },
                    height_calibrated: if self.calibration.is_some() {
                        Some(height)
                    } else {
                        None
                    },
                    area_calibrated: if self.calibration.is_some() {
                        Some(area)
                    } else {
                        None
                    },
                    unit,
                }
            })
            .collect();

        let export_data = ExportData {
            calibration: self.calibration.clone(),
            measurements,
            rectangle_measurements,
        };

        serde_json::to_string_pretty(&export_data).unwrap_or_default()
    }

    fn save_export(&self, format: &str) {
        let (content, extension, filter_name) = match format {
            "csv" => (self.export_csv(), "csv", "CSV"),
            "json" => (self.export_json(), "json", "JSON"),
            _ => return,
        };

        if let Some(path) = rfd::FileDialog::new()
            .add_filter(filter_name, &[extension])
            .save_file()
        {
            if let Err(e) = std::fs::write(&path, content) {
                eprintln!("Failed to save file: {}", e);
            }
        }
    }

    fn show_controls_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("controls_panel")
            .min_width(250.0)
            .show(ctx, |ui| {
                ui.heading("Sampo - 画像寸法測定");
                ui.separator();

                // ファイル操作
                ui.horizontal(|ui| {
                    if ui.button("画像を開く").clicked() {
                        self.open_file_dialog(ctx);
                    }
                });

                if let Some(path) = &self.image_path {
                    let filename = std::path::Path::new(path)
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| path.clone());
                    ui.label(format!("ファイル: {}", filename));
                }

                if let Some((w, h)) = self.image_dimensions {
                    ui.label(format!("サイズ: {}x{} px", w, h));
                }

                ui.separator();

                // ズーム
                ui.horizontal(|ui| {
                    ui.label("ズーム:");
                    ui.add(
                        egui::Slider::new(&mut self.zoom, 0.1..=5.0)
                            .logarithmic(true)
                            .suffix("x"),
                    );
                });
                ui.label("(画像上でピンチでもズーム可)");
                if ui.button("リセット").clicked() {
                    self.zoom = 1.0;
                }

                ui.separator();

                // 表示設定
                ui.heading("表示設定");
                ui.horizontal(|ui| {
                    ui.label("寸法文字色:");
                    ui.color_edit_button_srgba(&mut self.text_color);
                });
                ui.checkbox(&mut self.show_preview, "測定プレビューを表示");

                ui.separator();

                // キャリブレーション
                ui.heading("キャリブレーション");

                if let Some(cal) = &self.calibration {
                    ui.label(format!(
                        "設定済み: {:.2} px/{}",
                        cal.pixels_per_unit, cal.unit_name
                    ));
                    if ui.button("キャリブレーションをクリア").clicked() {
                        self.calibration = None;
                    }
                } else {
                    ui.label("未設定");
                }

                let calibrating_text = if self.is_calibrating {
                    "キャリブレーションをキャンセル"
                } else {
                    "キャリブレーションを開始"
                };
                if ui.button(calibrating_text).clicked() {
                    self.is_calibrating = !self.is_calibrating;
                    self.calibration_state = CalibrationState::Idle;
                }

                if self.is_calibrating {
                    match &self.calibration_state {
                        CalibrationState::Idle => {
                            ui.label("既知の長さの始点をクリック");
                        }
                        CalibrationState::FirstPointSelected(_) => {
                            ui.label("終点をクリック");
                        }
                        CalibrationState::WaitingForInput { distance_px, .. } => {
                            ui.label(format!("ピクセル距離: {:.1} px", distance_px));
                            ui.horizontal(|ui| {
                                ui.label("実寸法:");
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.calibration_input)
                                        .desired_width(60.0),
                                );
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.calibration_unit)
                                        .desired_width(40.0),
                                );
                            });
                            if ui.button("適用").clicked() {
                                if let Ok(real_distance) = self.calibration_input.parse::<f32>() {
                                    if real_distance > 0.0 {
                                        self.calibration = Some(Calibration {
                                            pixels_per_unit: distance_px / real_distance,
                                            unit_name: self.calibration_unit.clone(),
                                        });
                                        self.is_calibrating = false;
                                        self.calibration_state = CalibrationState::Idle;
                                        self.calibration_input.clear();
                                    }
                                }
                            }
                        }
                    }
                }

                ui.separator();

                // 測定操作
                ui.heading("測定");

                // モード切替
                ui.horizontal(|ui| {
                    ui.label("モード:");
                    ui.selectable_value(&mut self.measurement_mode, MeasurementMode::Line, "線分");
                    ui.selectable_value(
                        &mut self.measurement_mode,
                        MeasurementMode::Rectangle,
                        "矩形",
                    );
                });

                match &self.measurement_state {
                    MeasurementState::Idle => {
                        if !self.is_calibrating {
                            let mode_text = match self.measurement_mode {
                                MeasurementMode::Line => "線分",
                                MeasurementMode::Rectangle => "矩形",
                            };
                            ui.label(format!("画像をクリックして{}測定開始", mode_text));
                        }
                    }
                    MeasurementState::FirstPointSelected(p) => {
                        ui.label(format!("始点: ({:.0}, {:.0})", p.x, p.y));
                        let end_text = match self.measurement_mode {
                            MeasurementMode::Line => "終点をクリック",
                            MeasurementMode::Rectangle => "対角をクリック",
                        };
                        ui.label(end_text);
                        if ui.button("キャンセル").clicked() {
                            self.measurement_state = MeasurementState::Idle;
                        }
                    }
                }

                ui.separator();

                // 測定結果
                ui.heading("測定結果");

                egui::ScrollArea::vertical()
                    .max_height(200.0)
                    .show(ui, |ui| {
                        // 線分測定結果
                        let mut line_to_remove = None;
                        for (i, m) in self.measurements.iter().enumerate() {
                            let (distance, unit) =
                                m.distance_with_calibration(self.calibration.as_ref());
                            ui.horizontal(|ui| {
                                ui.label(format!("線#{}: {:.1} {}", i + 1, distance, unit));
                                if ui.small_button("x").clicked() {
                                    line_to_remove = Some(i);
                                }
                            });
                        }
                        if let Some(i) = line_to_remove {
                            self.measurements.remove(i);
                        }

                        // 矩形測定結果
                        let mut rect_to_remove = None;
                        for (i, rm) in self.rectangle_measurements.iter().enumerate() {
                            let (width, height, area, unit) =
                                rm.dimensions_with_calibration(self.calibration.as_ref());
                            let area_unit = if unit == "px" {
                                "px²".to_string()
                            } else {
                                format!("{}²", unit)
                            };
                            ui.horizontal(|ui| {
                                ui.label(format!(
                                    "矩#{}: {:.1}x{:.1} {}, {:.1} {}",
                                    i + 1,
                                    width,
                                    height,
                                    unit,
                                    area,
                                    area_unit
                                ));
                                if ui.small_button("x").clicked() {
                                    rect_to_remove = Some(i);
                                }
                            });
                        }
                        if let Some(i) = rect_to_remove {
                            self.rectangle_measurements.remove(i);
                        }
                    });

                if !self.measurements.is_empty() || !self.rectangle_measurements.is_empty() {
                    ui.horizontal(|ui| {
                        if ui.button("すべてクリア").clicked() {
                            self.measurements.clear();
                            self.rectangle_measurements.clear();
                        }
                    });
                }

                ui.separator();

                // エクスポート
                ui.heading("エクスポート");

                ui.horizontal(|ui| {
                    if ui.button("CSV").clicked() {
                        self.save_export("csv");
                    }
                    if ui.button("JSON").clicked() {
                        self.save_export("json");
                    }
                });
            });
    }
}

impl eframe::App for SampoApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.show_controls_panel(ctx);

        egui::CentralPanel::default().show(ctx, |ui| {
            // スクロールエリアの位置を取得
            let scroll_area_rect = ui.available_rect_before_wrap();

            // ピンチズームの処理（マウス位置を中心に）
            let zoom_delta = ui.input(|i| i.zoom_delta());
            let pointer_pos = ui.input(|i| i.pointer.hover_pos());

            let viewport_size = scroll_area_rect.size();

            // パディングはviewport_sizeと同じ
            let padding = viewport_size;

            // 画像読み込み時にスクロール位置をリセット（画像左上が画面左上に来るように）
            if self.needs_scroll_reset {
                self.scroll_offset = padding;
                self.needs_scroll_reset = false;
            }

            if zoom_delta != 1.0 && self.image_texture.is_some() {
                if let Some(pointer) = pointer_pos {
                    // ポインタがスクロールエリア内にあるか確認
                    if scroll_area_rect.contains(pointer) {
                        // ポインタの位置（スクロールエリア左上からの相対位置）
                        let pointer_rel = pointer - scroll_area_rect.min;

                        // ポインタが指しているコンテンツ上の位置
                        let content_pos = self.scroll_offset + pointer_rel;

                        // コンテンツ座標から画像座標を計算（パディングを引く）
                        let image_pos = content_pos - padding;

                        // 新しいズームを計算
                        let old_zoom = self.zoom;
                        let new_zoom = (old_zoom * zoom_delta).clamp(0.1, 5.0);
                        let zoom_ratio = new_zoom / old_zoom;

                        // 画像座標をズーム比率で拡大
                        let new_image_pos = image_pos * zoom_ratio;

                        // 新しいコンテンツ座標 = パディング + 新しい画像座標
                        let new_content_pos = padding + new_image_pos;

                        // 新しいスクロールオフセット = 新しいコンテンツ座標 - ポインタ相対位置
                        self.scroll_offset = new_content_pos - pointer_rel;

                        self.zoom = new_zoom;
                    }
                }
            }

            let scroll_output = egui::ScrollArea::both()
                .auto_shrink([false, false])
                .scroll_offset(self.scroll_offset)
                .show(ui, |ui| {
                    self.show_image_canvas(ui, viewport_size);
                });

            // ScrollAreaの実際のスクロール位置を同期
            self.scroll_offset = scroll_output.state.offset;
        });
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1024.0, 768.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Sampo - 画像寸法測定ツール",
        options,
        Box::new(|cc| Ok(Box::new(SampoApp::new(cc)))),
    )
}
