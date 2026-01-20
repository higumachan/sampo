use arboard::Clipboard;
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// スナップする角度の許容範囲（度）
const SNAP_ANGLE_TOLERANCE_DEG: f32 = 5.0;

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
            Some(cal) => (
                self.distance_px / cal.pixels_per_unit,
                cal.unit_name.clone(),
            ),
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
            None => (
                self.width_px,
                self.height_px,
                self.area_px,
                "px".to_string(),
            ),
        }
    }
}

/// キャリブレーション設定
#[derive(Clone, Serialize, Deserialize)]
struct Calibration {
    pixels_per_unit: f32,
    unit_name: String,
}

/// Undo/Redo 用の操作ログ
#[derive(Clone)]
enum Action {
    AddLine(Measurement),
    AddRect(RectangleMeasurement),
    RemoveLine(usize),
    RemoveRect(usize),
    SetCalibration(Option<Calibration>),
}

/// ログベースの履歴管理
#[derive(Default)]
struct History {
    actions: Vec<Action>,
    cursor: usize,
}

impl History {
    fn push_action(&mut self, action: Action) {
        if self.cursor < self.actions.len() {
            self.actions.truncate(self.cursor);
        }
        self.actions.push(action);
        self.cursor = self.actions.len();
    }

    fn can_undo(&self) -> bool {
        self.cursor > 0
    }

    fn can_redo(&self) -> bool {
        self.cursor < self.actions.len()
    }

    fn undo(&mut self) -> bool {
        if self.can_undo() {
            self.cursor -= 1;
            true
        } else {
            false
        }
    }

    fn redo(&mut self) -> bool {
        if self.can_redo() {
            self.cursor += 1;
            true
        } else {
            false
        }
    }

    fn rebuild_state(
        &self,
    ) -> (
        Vec<Measurement>,
        Vec<RectangleMeasurement>,
        Option<Calibration>,
    ) {
        let mut measurements = Vec::new();
        let mut rectangle_measurements = Vec::new();
        let mut calibration = None;

        for action in self.actions.iter().take(self.cursor) {
            match action {
                Action::AddLine(m) => measurements.push(m.clone()),
                Action::AddRect(r) => rectangle_measurements.push(r.clone()),
                Action::RemoveLine(index) => {
                    if *index < measurements.len() {
                        measurements.remove(*index);
                    }
                }
                Action::RemoveRect(index) => {
                    if *index < rectangle_measurements.len() {
                        rectangle_measurements.remove(*index);
                    }
                }
                Action::SetCalibration(cal) => {
                    calibration = cal.clone();
                }
            }
        }

        (measurements, rectangle_measurements, calibration)
    }

    fn reset_with_calibration(&mut self, calibration: Option<Calibration>) {
        self.actions.clear();
        self.cursor = 0;
        if let Some(cal) = calibration {
            self.actions.push(Action::SetCalibration(Some(cal)));
            self.cursor = self.actions.len();
        }
    }
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

/// 線分の終点をスナップ角度に合わせて調整する
/// start: 始点, end: 終点（スナップ前）
/// 戻り値: スナップ後の終点
fn snap_to_angle(start: egui::Pos2, end: egui::Pos2) -> egui::Pos2 {
    let delta = end - start;
    let distance = delta.length();
    if distance < 0.001 {
        return end;
    }

    // 角度を計算（ラジアン→度）
    let angle_rad = delta.y.atan2(delta.x);
    let angle_deg = angle_rad.to_degrees();

    // 0, 90, 180, -180, -90 にスナップ
    let snap_angles = [0.0_f32, 90.0, 180.0, -180.0, -90.0];

    for &snap_angle in &snap_angles {
        let diff = (angle_deg - snap_angle).abs();
        if diff <= SNAP_ANGLE_TOLERANCE_DEG {
            let snapped_rad = snap_angle.to_radians();
            return egui::pos2(
                start.x + distance * snapped_rad.cos(),
                start.y + distance * snapped_rad.sin(),
            );
        }
    }

    end // スナップしない場合はそのまま
}

/// 長さを指定した倍数にスナップする
/// length: 元の長さ, multiple: 倍数（0以下で無効）
/// 戻り値: スナップ後の長さ
fn snap_length_to_multiple(length: f32, multiple: f32) -> f32 {
    if multiple <= 0.0 {
        return length;
    }
    (length / multiple).round() * multiple
}

/// 線分の終点を長さが倍数になるように調整する
fn snap_line_length(start: egui::Pos2, end: egui::Pos2, multiple: f32) -> egui::Pos2 {
    if multiple <= 0.0 {
        return end;
    }
    let delta = end - start;
    let distance = delta.length();
    if distance < 0.001 {
        return end;
    }
    let snapped_distance = snap_length_to_multiple(distance, multiple);
    let direction = delta / distance;
    start + direction * snapped_distance
}

/// 矩形の対角点を幅・高さが倍数になるように調整する
fn snap_rect_dimensions(corner1: egui::Pos2, corner2: egui::Pos2, multiple: f32) -> egui::Pos2 {
    if multiple <= 0.0 {
        return corner2;
    }
    let dx = corner2.x - corner1.x;
    let dy = corner2.y - corner1.y;
    let snapped_width = snap_length_to_multiple(dx.abs(), multiple) * dx.signum();
    let snapped_height = snap_length_to_multiple(dy.abs(), multiple) * dy.signum();
    egui::pos2(corner1.x + snapped_width, corner1.y + snapped_height)
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
    is_ctrl_pressed: bool,
    length_snap_multiple: f32,
    history: History,
    /// 起動時に読み込む画像パス（テスト用）
    #[cfg(test)]
    pending_image_path: Option<PathBuf>,
    /// 起動時に追加する寸法（テスト用）
    #[cfg(test)]
    pending_measurements: Vec<(egui::Pos2, egui::Pos2)>,
    /// Undo回数（テスト用）
    #[cfg(test)]
    pending_undo_count: u32,
    /// Redo回数（テスト用）
    #[cfg(test)]
    pending_redo_count: u32,
    /// デバッグ用マウス位置（テスト用）- 画像座標で指定
    #[cfg(test)]
    debug_mouse_position: Option<egui::Pos2>,
    /// Ctrl押下をシミュレート（テスト用）
    #[cfg(test)]
    debug_ctrl_pressed: bool,
    /// プレビュー表示用に測定の始点を選択状態にする（テスト用）
    #[cfg(test)]
    pending_first_point: Option<egui::Pos2>,
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
            text_color: egui::Color32::BLACK,
            scroll_offset: egui::Vec2::ZERO,
            needs_scroll_reset: false,
            show_preview: true,
            current_mouse_image_pos: None,
            is_ctrl_pressed: false,
            length_snap_multiple: 1.0,
            history: History::default(),
            #[cfg(test)]
            pending_image_path: None,
            #[cfg(test)]
            pending_measurements: Vec::new(),
            #[cfg(test)]
            pending_undo_count: 0,
            #[cfg(test)]
            pending_redo_count: 0,
            #[cfg(test)]
            debug_mouse_position: None,
            #[cfg(test)]
            debug_ctrl_pressed: false,
            #[cfg(test)]
            pending_first_point: None,
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

    /// テスト用コンストラクタ：初期画像パスと寸法を指定可能
    #[cfg(test)]
    fn new_for_test(
        cc: &eframe::CreationContext<'_>,
        image_path: Option<PathBuf>,
        measurements: Vec<(egui::Pos2, egui::Pos2)>,
    ) -> Self {
        Self::new_for_test_with_snap(cc, image_path, measurements, 0.0)
    }

    /// テスト用コンストラクタ（スナップ倍数を指定可能）
    #[cfg(test)]
    fn new_for_test_with_snap(
        cc: &eframe::CreationContext<'_>,
        image_path: Option<PathBuf>,
        measurements: Vec<(egui::Pos2, egui::Pos2)>,
        length_snap_multiple: f32,
    ) -> Self {
        egui_extras::install_image_loaders(&cc.egui_ctx);

        // NotoSans JP フォントを設定（通常のnewと同様）
        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(
            "NotoSansJP".to_owned(),
            std::sync::Arc::new(egui::FontData::from_static(include_bytes!(
                "../assets/NotoSansJP-Regular.ttf"
            ))),
        );
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

        let mut app = Self::default();
        app.pending_image_path = image_path;
        app.pending_measurements = measurements;
        app.length_snap_multiple = length_snap_multiple;
        app
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
                self.history = History::default();
            }
            Err(e) => {
                eprintln!("Failed to load image: {}", e);
            }
        }
    }

    fn load_image_from_rgba(
        &mut self,
        ctx: &egui::Context,
        width: u32,
        height: u32,
        rgba_data: Vec<u8>,
        source_name: &str,
    ) {
        let color_image =
            egui::ColorImage::from_rgba_unmultiplied([width as usize, height as usize], &rgba_data);

        let texture = ctx.load_texture(source_name, color_image, egui::TextureOptions::LINEAR);

        self.image_texture = Some(texture);
        self.image_dimensions = Some((width, height));
        self.image_path = Some(source_name.to_string());
        self.measurements.clear();
        self.rectangle_measurements.clear();
        self.measurement_state = MeasurementState::Idle;
        self.calibration = None;
        self.calibration_state = CalibrationState::Idle;
        self.is_calibrating = false;
        self.zoom = 1.0;
        self.needs_scroll_reset = true;
        self.history = History::default();
    }

    fn paste_from_clipboard(&mut self, ctx: &egui::Context) {
        match Clipboard::new() {
            Ok(mut clipboard) => match clipboard.get_image() {
                Ok(img_data) => {
                    // arboard::ImageData の RGBA データを取得
                    let width = img_data.width as u32;
                    let height = img_data.height as u32;
                    let rgba_data = img_data.bytes.into_owned();

                    self.load_image_from_rgba(
                        ctx,
                        width,
                        height,
                        rgba_data,
                        "[クリップボードから貼り付け]",
                    );
                }
                Err(e) => {
                    eprintln!("クリップボードに画像がありません: {}", e);
                }
            },
            Err(e) => {
                eprintln!("クリップボードへのアクセスに失敗: {}", e);
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

    fn rebuild_from_history(&mut self) {
        let (measurements, rectangle_measurements, calibration) = self.history.rebuild_state();
        self.measurements = measurements;
        self.rectangle_measurements = rectangle_measurements;
        self.calibration = calibration;
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
                    // 角度スナップ（Ctrl）
                    let angle_snapped = if self.is_ctrl_pressed {
                        snap_to_angle(start, image_pos)
                    } else {
                        image_pos
                    };
                    // 倍数スナップ
                    let end_pos = snap_line_length(start, angle_snapped, self.length_snap_multiple);
                    let distance_px = start.distance(end_pos);
                    self.calibration_state = CalibrationState::WaitingForInput {
                        start,
                        end: end_pos,
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
                            let angle_snapped = if self.is_ctrl_pressed {
                                snap_to_angle(*start, image_pos)
                            } else {
                                image_pos
                            };
                            let end_pos =
                                snap_line_length(*start, angle_snapped, self.length_snap_multiple);
                            let measurement = Measurement::new(*start, end_pos);
                            self.history.push_action(Action::AddLine(measurement));
                            self.rebuild_from_history();
                        }
                        MeasurementMode::Rectangle => {
                            let end_pos =
                                snap_rect_dimensions(*start, image_pos, self.length_snap_multiple);
                            let rect_measurement = RectangleMeasurement::new(*start, end_pos);
                            self.history.push_action(Action::AddRect(rect_measurement));
                            self.rebuild_from_history();
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
                    let preview_color = egui::Color32::from_rgba_unmultiplied(255, 255, 0, 150);
                    let preview_stroke = egui::Stroke::new(1.5, preview_color);

                    match self.measurement_mode {
                        MeasurementMode::Line => {
                            // 角度スナップ適用（Ctrl）
                            let angle_snapped = if self.is_ctrl_pressed {
                                snap_to_angle(*start, mouse_pos)
                            } else {
                                mouse_pos
                            };
                            // 倍数スナップ適用
                            let effective_mouse_pos =
                                snap_line_length(*start, angle_snapped, self.length_snap_multiple);
                            let effective_mouse_screen =
                                self.image_to_screen(effective_mouse_pos, image_rect);

                            // 線分のプレビュー
                            painter.line_segment(
                                [start_screen, effective_mouse_screen],
                                preview_stroke,
                            );
                            painter.circle_filled(
                                effective_mouse_screen,
                                point_radius * 0.7,
                                preview_color,
                            );

                            // 距離のプレビュー表示
                            let distance_px = start.distance(effective_mouse_pos);
                            let (distance, unit) = match &self.calibration {
                                Some(cal) => {
                                    (distance_px / cal.pixels_per_unit, cal.unit_name.clone())
                                }
                                None => (distance_px, "px".to_string()),
                            };
                            let midpoint =
                                start_screen + (effective_mouse_screen - start_screen) * 0.5;
                            painter.text(
                                midpoint + egui::vec2(0.0, -15.0),
                                egui::Align2::CENTER_BOTTOM,
                                format!("{:.1} {}", distance, unit),
                                egui::FontId::default(),
                                self.text_color,
                            );
                        }
                        MeasurementMode::Rectangle => {
                            // 倍数スナップ適用
                            let effective_mouse_pos =
                                snap_rect_dimensions(*start, mouse_pos, self.length_snap_multiple);
                            let effective_mouse_screen =
                                self.image_to_screen(effective_mouse_pos, image_rect);

                            // 矩形のプレビュー
                            let min_x = start_screen.x.min(effective_mouse_screen.x);
                            let max_x = start_screen.x.max(effective_mouse_screen.x);
                            let min_y = start_screen.y.min(effective_mouse_screen.y);
                            let max_y = start_screen.y.max(effective_mouse_screen.y);

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
                            let width_px = (effective_mouse_pos.x - start.x).abs();
                            let height_px = (effective_mouse_pos.y - start.y).abs();
                            let area_px = width_px * height_px;

                            let (width, height, area, unit) = match &self.calibration {
                                Some(cal) => {
                                    let w = width_px / cal.pixels_per_unit;
                                    let h = height_px / cal.pixels_per_unit;
                                    let a = area_px / (cal.pixels_per_unit * cal.pixels_per_unit);
                                    (w, h, a, cal.unit_name.clone())
                                }
                                None => (width_px, height_px, area_px, "px".to_string()),
                            };

                            // 幅ラベル
                            let width_pos =
                                egui::pos2((top_left.x + top_right.x) / 2.0, min_y - 15.0);
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

                // キャリブレーションのプレビュー描画
                if self.show_preview {
                    if let Some(mouse_pos) = self.current_mouse_image_pos {
                        let preview_color =
                            egui::Color32::from_rgba_unmultiplied(100, 200, 255, 150);
                        let preview_stroke = egui::Stroke::new(1.5, preview_color);

                        // 角度スナップ（Ctrl）
                        let angle_snapped = if self.is_ctrl_pressed {
                            snap_to_angle(*start, mouse_pos)
                        } else {
                            mouse_pos
                        };
                        // 倍数スナップ
                        let effective_mouse_pos =
                            snap_line_length(*start, angle_snapped, self.length_snap_multiple);
                        let effective_mouse_screen =
                            self.image_to_screen(effective_mouse_pos, image_rect);

                        // 線分のプレビュー
                        painter
                            .line_segment([start_screen, effective_mouse_screen], preview_stroke);
                        painter.circle_filled(
                            effective_mouse_screen,
                            point_radius * 0.7,
                            preview_color,
                        );

                        // 距離のプレビュー表示
                        let distance_px = start.distance(effective_mouse_pos);
                        let midpoint = start_screen + (effective_mouse_screen - start_screen) * 0.5;
                        painter.text(
                            midpoint + egui::vec2(0.0, -15.0),
                            egui::Align2::CENTER_BOTTOM,
                            format!("{:.1} px", distance_px),
                            egui::FontId::default(),
                            self.text_color,
                        );
                    }
                }
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

        // テスト用：デバッグポインタの描画
        #[cfg(test)]
        if let Some(debug_pos) = self.debug_mouse_position {
            let debug_screen = self.image_to_screen(debug_pos, image_rect);
            // 真のマウス位置を青い十字で表示
            let cross_size = 15.0;
            let cross_color = egui::Color32::from_rgb(0, 100, 255);
            let cross_stroke = egui::Stroke::new(2.0, cross_color);
            // 水平線
            painter.line_segment(
                [
                    egui::pos2(debug_screen.x - cross_size, debug_screen.y),
                    egui::pos2(debug_screen.x + cross_size, debug_screen.y),
                ],
                cross_stroke,
            );
            // 垂直線
            painter.line_segment(
                [
                    egui::pos2(debug_screen.x, debug_screen.y - cross_size),
                    egui::pos2(debug_screen.x, debug_screen.y + cross_size),
                ],
                cross_stroke,
            );
            // 座標ラベル
            painter.text(
                debug_screen + egui::vec2(20.0, -10.0),
                egui::Align2::LEFT_BOTTOM,
                format!("({:.0}, {:.0})", debug_pos.x, debug_pos.y),
                egui::FontId::default(),
                cross_color,
            );
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
            // テスト用：debug_mouse_positionが設定されている場合は上書きしない
            #[cfg(test)]
            if self.debug_mouse_position.is_none() {
                self.current_mouse_image_pos = hover_pos.map(|pos| self.screen_to_image(pos, rect));
            }
            #[cfg(not(test))]
            {
                self.current_mouse_image_pos = hover_pos.map(|pos| self.screen_to_image(pos, rect));
            }

            if let Some(pointer_pos) = clicked_pos {
                self.handle_canvas_click(pointer_pos, rect);
            }

            // 測定線を描画（別のPainterを使用）
            let painter = ui.painter_at(rect);
            self.draw_measurements(&painter, rect);
        } else {
            // テスト用：debug_mouse_positionが設定されている場合は上書きしない
            #[cfg(test)]
            if self.debug_mouse_position.is_none() {
                self.current_mouse_image_pos = None;
            }
            #[cfg(not(test))]
            {
                self.current_mouse_image_pos = None;
            }
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

                // Undo / Redo
                ui.horizontal(|ui| {
                    let can_undo = self.history.can_undo();
                    let can_redo = self.history.can_redo();
                    if ui.add_enabled(can_undo, egui::Button::new("Undo")).clicked() {
                        if self.history.undo() {
                            self.rebuild_from_history();
                        }
                    }
                    if ui.add_enabled(can_redo, egui::Button::new("Redo")).clicked() {
                        if self.history.redo() {
                            self.rebuild_from_history();
                        }
                    }
                });

                ui.separator();

                // ファイル操作
                ui.horizontal(|ui| {
                    if ui.button("画像を開く").clicked() {
                        self.open_file_dialog(ctx);
                    }
                    if ui.button("貼り付け").clicked() {
                        self.paste_from_clipboard(ctx);
                    }
                });
                ui.label("(Ctrl/Cmd+V でも貼り付け可)");

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
                        self.history.push_action(Action::SetCalibration(None));
                        self.rebuild_from_history();
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
                                        let calibration = Calibration {
                                            pixels_per_unit: distance_px / real_distance,
                                            unit_name: self.calibration_unit.clone(),
                                        };
                                        self.history
                                            .push_action(Action::SetCalibration(Some(calibration)));
                                        self.rebuild_from_history();
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

                if self.measurement_mode == MeasurementMode::Line {
                    ui.label("(Ctrl押下で水平/垂直スナップ)");
                }

                ui.horizontal(|ui| {
                    ui.label("長さスナップ:");
                    ui.add(
                        egui::DragValue::new(&mut self.length_snap_multiple)
                            .speed(0.1)
                            .range(0.0..=100.0)
                            .suffix(" px"),
                    );
                });
                ui.label("(0で無効)");

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
                            self.history.push_action(Action::RemoveLine(i));
                            self.rebuild_from_history();
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
                            self.history.push_action(Action::RemoveRect(i));
                            self.rebuild_from_history();
                        }
                    });

                if !self.measurements.is_empty() || !self.rectangle_measurements.is_empty() {
                    ui.horizontal(|ui| {
                        if ui.button("すべてクリア").clicked() {
                            self.measurements.clear();
                            self.rectangle_measurements.clear();
                            self.history
                                .reset_with_calibration(self.calibration.clone());
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
        // テスト用：起動時に指定された画像を読み込む
        #[cfg(test)]
        if let Some(path) = self.pending_image_path.take() {
            self.load_image(ctx, &path);
        }

        // テスト用：起動時に指定された寸法を追加
        #[cfg(test)]
        if !self.pending_measurements.is_empty() {
            let measurements = std::mem::take(&mut self.pending_measurements);
            for (start, end) in measurements {
                let measurement = Measurement::new(start, end);
                self.history.push_action(Action::AddLine(measurement));
            }
            self.rebuild_from_history();
        }

        // テスト用：Undo操作を実行
        #[cfg(test)]
        while self.pending_undo_count > 0 {
            if self.history.undo() {
                self.rebuild_from_history();
            }
            self.pending_undo_count -= 1;
        }

        // テスト用：Redo操作を実行
        #[cfg(test)]
        while self.pending_redo_count > 0 {
            if self.history.redo() {
                self.rebuild_from_history();
            }
            self.pending_redo_count -= 1;
        }

        // テスト用：測定の始点選択状態を設定（プレビューテスト用）
        #[cfg(test)]
        if let Some(start) = self.pending_first_point.take() {
            self.measurement_state = MeasurementState::FirstPointSelected(start);
        }

        // テスト用：デバッグマウス位置をcurrent_mouse_image_posに設定
        #[cfg(test)]
        if let Some(debug_pos) = self.debug_mouse_position {
            self.current_mouse_image_pos = Some(debug_pos);
        }

        // テスト用：Ctrl押下状態をシミュレート
        #[cfg(test)]
        if self.debug_ctrl_pressed {
            self.is_ctrl_pressed = true;
        }

        // Ctrlキーの状態を取得（テストでdebug_ctrl_pressedがfalseの場合も含む）
        #[cfg(not(test))]
        {
            self.is_ctrl_pressed = ctx.input(|i| i.modifiers.ctrl);
        }
        #[cfg(test)]
        if !self.debug_ctrl_pressed {
            self.is_ctrl_pressed = ctx.input(|i| i.modifiers.ctrl);
        }

        // キーボードショートカット: Ctrl+V / Cmd+V でクリップボードから貼り付け
        let paste_shortcut = ctx.input(|i| i.key_pressed(egui::Key::V) && i.modifiers.command);
        if paste_shortcut {
            self.paste_from_clipboard(ctx);
        }

        // Undo/Redo ショートカット: Ctrl/Cmd+Z, Shift+Ctrl/Cmd+Z
        let undo_shortcut = ctx
            .input(|i| i.key_pressed(egui::Key::Z) && i.modifiers.command && !i.modifiers.shift);
        let redo_shortcut = ctx
            .input(|i| i.key_pressed(egui::Key::Z) && i.modifiers.command && i.modifiers.shift);
        if undo_shortcut && self.history.undo() {
            self.rebuild_from_history();
        }
        if redo_shortcut && self.history.redo() {
            self.rebuild_from_history();
        }

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

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::Harness;
    use std::path::PathBuf;

    /// テスト用ハーネスを作成するヘルパー関数
    fn create_test_harness(
        measurements: Vec<(egui::Pos2, egui::Pos2)>,
    ) -> Harness<'static, SampoApp> {
        let image_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/lenna.png");
        Harness::builder()
            .with_size(egui::vec2(1024.0, 768.0))
            .build_eframe(|cc| SampoApp::new_for_test(cc, Some(image_path), measurements))
    }

    #[test]
    fn test_load_image_and_add_dimension() {
        // 追加する寸法を準備（画像座標）
        let start = egui::pos2(100.0, 100.0);
        let end = egui::pos2(200.0, 150.0);
        let measurements = vec![(start, end)];

        let mut harness = create_test_harness(measurements);

        // 初回のupdateで画像と寸法が読み込まれる
        harness.run();

        // 画像が読み込まれたことを確認
        assert!(
            harness.state().image_texture.is_some(),
            "画像が読み込まれていません"
        );
        assert!(
            harness.state().image_dimensions.is_some(),
            "画像サイズが設定されていません"
        );

        // 検証：寸法が1つ追加されていることを確認
        assert_eq!(
            harness.state().measurements.len(),
            1,
            "寸法が1つ追加されているべき"
        );

        // 追加された寸法の検証
        let measurement = &harness.state().measurements[0];
        assert!(
            (measurement.start.0 - 100.0).abs() < 0.1,
            "始点Xが約100であるべき（実際: {:.1}）",
            measurement.start.0
        );
        assert!(
            (measurement.start.1 - 100.0).abs() < 0.1,
            "始点Yが約100であるべき（実際: {:.1}）",
            measurement.start.1
        );
        assert!(
            (measurement.end.0 - 200.0).abs() < 0.1,
            "終点Xが約200であるべき（実際: {:.1}）",
            measurement.end.0
        );
        assert!(
            (measurement.end.1 - 150.0).abs() < 0.1,
            "終点Yが約150であるべき（実際: {:.1}）",
            measurement.end.1
        );

        // 距離の検証（100^2 + 50^2 = 10000 + 2500 = 12500, sqrt(12500) ≈ 111.8）
        let expected_distance = ((100.0_f32).powi(2) + (50.0_f32).powi(2)).sqrt();
        assert!(
            (measurement.distance_px - expected_distance).abs() < 0.1,
            "距離が約{:.1}pxであるべき（実際: {:.1}）",
            expected_distance,
            measurement.distance_px
        );

        // スナップショットテスト
        harness.snapshot("load_image_and_add_dimension");
    }

    /// シナリオ: 寸法を1つ追加 → Undo → 寸法が消える
    #[test]
    fn test_undo_single_measurement() {
        // 1つの寸法を追加
        let measurements = vec![(egui::pos2(100.0, 100.0), egui::pos2(200.0, 150.0))];
        let mut harness = create_test_harness(measurements);

        // 初回のupdateで寸法が追加される
        harness.run();
        assert_eq!(
            harness.state().measurements.len(),
            1,
            "初期状態: 寸法が1つあるべき"
        );

        // Undoを実行するためにpending_undo_countを設定
        harness.state_mut().pending_undo_count = 1;
        harness.run();

        // 検証: 寸法が消えている
        assert_eq!(
            harness.state().measurements.len(),
            0,
            "Undo後: 寸法が0になるべき"
        );

        // スナップショット: Undo後の状態
        harness.snapshot("undo_single_measurement");
    }

    /// シナリオ: 寸法を追加 → Undo → Redo → 寸法が復活
    #[test]
    fn test_redo_after_undo() {
        // 1つの寸法を追加
        let measurements = vec![(egui::pos2(100.0, 100.0), egui::pos2(200.0, 150.0))];
        let mut harness = create_test_harness(measurements);

        // 初回のupdate
        harness.run();
        assert_eq!(harness.state().measurements.len(), 1, "初期状態: 1つ");

        // Undo
        harness.state_mut().pending_undo_count = 1;
        harness.run();
        assert_eq!(harness.state().measurements.len(), 0, "Undo後: 0");

        // Redo
        harness.state_mut().pending_redo_count = 1;
        harness.run();
        assert_eq!(harness.state().measurements.len(), 1, "Redo後: 1つ復活");

        // 復活した寸法の検証
        let measurement = &harness.state().measurements[0];
        assert!(
            (measurement.start.0 - 100.0).abs() < 0.1,
            "Redo後の始点Xが100であるべき"
        );
        assert!(
            (measurement.end.0 - 200.0).abs() < 0.1,
            "Redo後の終点Xが200であるべき"
        );

        // スナップショット: Redo後の状態
        harness.snapshot("redo_after_undo");
    }

    /// シナリオ: 複数の寸法を追加 → 複数回Undo → 複数回Redo
    #[test]
    fn test_multiple_undo_redo() {
        // 3つの寸法を追加
        let measurements = vec![
            (egui::pos2(100.0, 100.0), egui::pos2(150.0, 100.0)), // 水平線 50px
            (egui::pos2(200.0, 100.0), egui::pos2(200.0, 180.0)), // 垂直線 80px
            (egui::pos2(300.0, 100.0), egui::pos2(400.0, 200.0)), // 斜め線
        ];
        let mut harness = create_test_harness(measurements);

        // 初回のupdate
        harness.run();
        assert_eq!(harness.state().measurements.len(), 3, "初期状態: 3つ");

        // 1回Undo → 2つになる
        harness.state_mut().pending_undo_count = 1;
        harness.run();
        assert_eq!(harness.state().measurements.len(), 2, "Undo 1回後: 2つ");

        // スナップショット: 2つの寸法
        harness.snapshot("multiple_undo_redo_step1_after_undo");

        // もう1回Undo → 1つになる
        harness.state_mut().pending_undo_count = 1;
        harness.run();
        assert_eq!(harness.state().measurements.len(), 1, "Undo 2回後: 1つ");

        // 残った寸法は最初のもの（水平線 50px）
        let remaining = &harness.state().measurements[0];
        let expected_dist = 50.0;
        assert!(
            (remaining.distance_px - expected_dist).abs() < 0.1,
            "残った寸法は50pxであるべき（実際: {:.1}）",
            remaining.distance_px
        );

        // スナップショット: 1つの寸法
        harness.snapshot("multiple_undo_redo_step2_after_undo");

        // Redo 1回 → 2つに戻る
        harness.state_mut().pending_redo_count = 1;
        harness.run();
        assert_eq!(harness.state().measurements.len(), 2, "Redo 1回後: 2つ");

        // 2番目の寸法が復活（垂直線 80px）
        let second = &harness.state().measurements[1];
        let expected_dist = 80.0;
        assert!(
            (second.distance_px - expected_dist).abs() < 0.1,
            "復活した寸法は80pxであるべき（実際: {:.1}）",
            second.distance_px
        );

        // Redo もう1回 → 3つに戻る
        harness.state_mut().pending_redo_count = 1;
        harness.run();
        assert_eq!(harness.state().measurements.len(), 3, "Redo 2回後: 3つ");

        // スナップショット: 全て復活
        harness.snapshot("multiple_undo_redo_step3_all_restored");
    }

    /// シナリオ: Undo後に新しい操作 → Redoできなくなる
    #[test]
    fn test_undo_then_new_action_clears_redo() {
        // 2つの寸法を追加
        let measurements = vec![
            (egui::pos2(100.0, 100.0), egui::pos2(200.0, 100.0)), // 100px
            (egui::pos2(100.0, 200.0), egui::pos2(200.0, 200.0)), // 100px
        ];
        let mut harness = create_test_harness(measurements);

        // 初回のupdate
        harness.run();
        assert_eq!(harness.state().measurements.len(), 2, "初期状態: 2つ");

        // Undo → 1つになる
        harness.state_mut().pending_undo_count = 1;
        harness.run();
        assert_eq!(harness.state().measurements.len(), 1, "Undo後: 1つ");

        // Redoできるか確認
        assert!(harness.state().history.can_redo(), "Redo可能であるべき");

        // 新しい寸法を追加（これによりRedo履歴がクリアされる）
        harness.state_mut().pending_measurements =
            vec![(egui::pos2(300.0, 300.0), egui::pos2(400.0, 300.0))];
        harness.run();

        // 2つになる（Undo前の1つ + 新しい1つ）
        assert_eq!(
            harness.state().measurements.len(),
            2,
            "新しい操作後: 2つ"
        );

        // Redoできなくなっている
        assert!(
            !harness.state().history.can_redo(),
            "新しい操作後はRedoできないべき"
        );

        // スナップショット
        harness.snapshot("undo_then_new_action_clears_redo");
    }

    // ========================================
    // スナップモードのシナリオテスト
    // ========================================

    /// スナップ対応テスト用ハーネスを作成するヘルパー関数
    fn create_test_harness_with_snap(
        measurements: Vec<(egui::Pos2, egui::Pos2)>,
        length_snap_multiple: f32,
    ) -> Harness<'static, SampoApp> {
        let image_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/lenna.png");
        Harness::builder()
            .with_size(egui::vec2(1024.0, 768.0))
            .build_eframe(|cc| {
                SampoApp::new_for_test_with_snap(cc, Some(image_path), measurements, length_snap_multiple)
            })
    }

    /// シナリオ: 角度スナップのプレビュー表示
    /// - 始点(100, 100)を選択
    /// - マウスを(200, 103)に移動（ほぼ水平）
    /// - Ctrlを押した状態でプレビュー表示
    /// - 青い十字で真のマウス位置、黄色いプレビュー線で水平にスナップされた線を表示
    #[test]
    fn test_angle_snap_preview() {
        let mut harness = create_test_harness_with_snap(vec![], 0.0);

        // 初期化
        harness.run();

        // 始点を選択状態にする
        harness.state_mut().pending_first_point = Some(egui::pos2(100.0, 100.0));
        // マウス位置を設定（ほぼ水平だが、少しずれている）
        harness.state_mut().debug_mouse_position = Some(egui::pos2(200.0, 103.0));
        // Ctrlを押した状態
        harness.state_mut().debug_ctrl_pressed = true;

        harness.run();

        // 始点が選択されていることを確認
        assert!(
            matches!(
                harness.state().measurement_state,
                MeasurementState::FirstPointSelected(_)
            ),
            "始点選択状態であるべき"
        );

        // Ctrlが押されていることを確認
        assert!(
            harness.state().is_ctrl_pressed,
            "Ctrl押下状態であるべき"
        );

        // スナップショット: 角度スナップのプレビュー
        // - 青い十字: 真のマウス位置(200, 103)
        // - 黄色いプレビュー線: (100, 100)から水平にスナップされた線
        harness.snapshot("angle_snap_preview");
    }

    /// シナリオ: 角度スナップなしのプレビュー表示（比較用）
    /// - Ctrl押下なしで斜めの線のプレビュー
    #[test]
    fn test_no_angle_snap_preview() {
        let mut harness = create_test_harness_with_snap(vec![], 0.0);

        // 初期化
        harness.run();

        // 始点を選択状態にする
        harness.state_mut().pending_first_point = Some(egui::pos2(100.0, 100.0));
        // マウス位置を設定（斜め）
        harness.state_mut().debug_mouse_position = Some(egui::pos2(200.0, 130.0));
        // Ctrlは押していない
        harness.state_mut().debug_ctrl_pressed = false;

        harness.run();

        // スナップショット: 角度スナップなしのプレビュー
        // - 青い十字: 真のマウス位置(200, 130)
        // - 黄色いプレビュー線: 斜めのまま
        harness.snapshot("no_angle_snap_preview");
    }

    /// シナリオ: 長さスナップが有効な状態で寸法を追加
    /// - length_snap_multiple = 10.0 に設定
    /// - (100, 100)から(143, 100)の線を追加（長さ43px）
    /// - 長さは40pxにスナップされる
    #[test]
    fn test_length_snap() {
        // 長さスナップを10pxに設定
        let mut harness = create_test_harness_with_snap(vec![], 10.0);

        // 初期化
        harness.run();

        // 始点を選択状態にする
        harness.state_mut().pending_first_point = Some(egui::pos2(100.0, 100.0));
        // マウス位置（43px離れている）
        harness.state_mut().debug_mouse_position = Some(egui::pos2(143.0, 100.0));

        harness.run();

        // スナップショット: 長さスナップのプレビュー
        // プレビュー線は40pxにスナップされているはず
        harness.snapshot("length_snap_preview");

        // 実際に寸法を追加（pending_measurementsを使うとスナップが適用されないので、
        // handle_canvas_clickをシミュレートするために直接追加）
        // まずは状態をリセット
        let start = egui::pos2(100.0, 100.0);
        let end = egui::pos2(143.0, 100.0);
        // 長さスナップを適用
        let snapped_end = snap_line_length(start, end, 10.0);
        let measurement = Measurement::new(start, snapped_end);

        // 寸法を追加
        harness.state_mut().history.push_action(Action::AddLine(measurement.clone()));
        harness.state_mut().rebuild_from_history();
        harness.state_mut().measurement_state = MeasurementState::Idle;
        harness.state_mut().debug_mouse_position = None;

        harness.run();

        // 追加された寸法の検証
        assert_eq!(
            harness.state().measurements.len(),
            1,
            "寸法が1つ追加されているべき"
        );

        // スナップされた距離を検証（40pxになるはず）
        let added_measurement = &harness.state().measurements[0];
        assert!(
            (added_measurement.distance_px - 40.0).abs() < 0.1,
            "長さは40pxにスナップされるべき（実際: {:.1}）",
            added_measurement.distance_px
        );

        // 終点X座標を検証（100 + 40 = 140）
        assert!(
            (added_measurement.end.0 - 140.0).abs() < 0.1,
            "終点Xは140であるべき（実際: {:.1}）",
            added_measurement.end.0
        );

        harness.snapshot("length_snap_result");
    }

    /// シナリオ: 角度スナップ + 長さスナップの複合
    /// - Ctrl押下で水平にスナップ
    /// - 長さも10pxの倍数にスナップ
    #[test]
    fn test_angle_and_length_combined_snap() {
        // 長さスナップを10pxに設定
        let mut harness = create_test_harness_with_snap(vec![], 10.0);

        // 初期化
        harness.run();

        // 始点を選択状態にする
        harness.state_mut().pending_first_point = Some(egui::pos2(100.0, 100.0));
        // マウス位置（斜め、長さも半端）
        harness.state_mut().debug_mouse_position = Some(egui::pos2(153.0, 104.0));
        // Ctrl押下で角度スナップ
        harness.state_mut().debug_ctrl_pressed = true;

        harness.run();

        // スナップショット: 角度+長さスナップのプレビュー
        // - 青い十字: 真のマウス位置(153, 104)
        // - 黄色いプレビュー線: 水平にスナップ、長さも50pxにスナップ
        harness.snapshot("angle_and_length_snap_preview");

        // 実際に角度+長さスナップを適用した寸法を追加
        let start = egui::pos2(100.0, 100.0);
        let raw_end = egui::pos2(153.0, 104.0);
        // 角度スナップを適用
        let angle_snapped = snap_to_angle(start, raw_end);
        // 長さスナップを適用
        let snapped_end = snap_line_length(start, angle_snapped, 10.0);
        let measurement = Measurement::new(start, snapped_end);

        harness.state_mut().history.push_action(Action::AddLine(measurement.clone()));
        harness.state_mut().rebuild_from_history();
        harness.state_mut().measurement_state = MeasurementState::Idle;
        harness.state_mut().debug_mouse_position = None;
        harness.state_mut().debug_ctrl_pressed = false;

        harness.run();

        // 検証
        assert_eq!(harness.state().measurements.len(), 1, "寸法が1つ追加されているべき");

        let added = &harness.state().measurements[0];
        // 角度スナップで水平になっているはず（Y座標が同じ）
        assert!(
            (added.end.1 - added.start.1).abs() < 0.1,
            "角度スナップで水平になるべき（Y差: {:.1}）",
            (added.end.1 - added.start.1).abs()
        );
        // 長さスナップで10の倍数になっているはず
        let distance_mod_10 = added.distance_px % 10.0;
        assert!(
            distance_mod_10 < 0.1 || (10.0 - distance_mod_10) < 0.1,
            "長さは10の倍数であるべき（実際: {:.1}）",
            added.distance_px
        );

        harness.snapshot("angle_and_length_snap_result");
    }

    /// シナリオ: 垂直方向への角度スナップ
    /// - 始点(100, 100)を選択
    /// - マウスを(103, 200)に移動（ほぼ垂直）
    /// - Ctrlを押した状態でプレビュー表示
    #[test]
    fn test_angle_snap_vertical() {
        let mut harness = create_test_harness_with_snap(vec![], 0.0);

        // 初期化
        harness.run();

        // 始点を選択状態にする
        harness.state_mut().pending_first_point = Some(egui::pos2(100.0, 100.0));
        // マウス位置を設定（ほぼ垂直）
        harness.state_mut().debug_mouse_position = Some(egui::pos2(103.0, 200.0));
        // Ctrlを押した状態
        harness.state_mut().debug_ctrl_pressed = true;

        harness.run();

        // スナップショット: 垂直方向への角度スナップ
        harness.snapshot("angle_snap_vertical");
    }
}
