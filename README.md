# Sampo

ラスタ画像上で線分や矩形の寸法を測定するデスクトップアプリケーションです。

![Rust](https://img.shields.io/badge/Rust-2024-orange)
![egui](https://img.shields.io/badge/GUI-egui-blue)

https://i.gyazo.com/b813fa400b420f2f58db6bd927caa6ac.mp4

## 機能

### 画像の読み込み

- **ファイルから開く**: PNG, JPEG, GIF, BMP, WebP形式に対応
- **クリップボードから貼り付け**: Ctrl+V (macOSはCmd+V) または「貼り付け」ボタン

### 測定モード

#### 線分測定
画像上の2点をクリックして、その間の距離を測定します。

#### 矩形測定
画像上の2点（対角）をクリックして、矩形の幅・高さ・面積を測定します。

### スナップ機能

- **角度スナップ**: Ctrlキーを押しながら測定すると、水平・垂直方向（0°, 90°, 180°, -90°）にスナップ
- **長さスナップ**: 設定した倍数に長さをスナップ（デフォルト: 1px = 整数値スナップ）

### キャリブレーション

ピクセル単位を実世界の単位（mm, cm など）に変換できます。

1. 「キャリブレーションを開始」をクリック
2. 既知の長さの線分を画像上で指定
3. 実際の寸法と単位を入力して「適用」

キャリブレーション後は、すべての測定結果が設定した単位で表示されます。

### 表示設定

- **ズーム**: スライダーまたはピンチジェスチャー（マウス位置を中心にズーム）
- **寸法文字色**: 背景に合わせて文字色をカスタマイズ
- **測定プレビュー**: 測定中の線分・矩形をリアルタイム表示

### 測定結果の管理

- 測定結果は一覧表示され、個別に削除可能
- 「すべてクリア」で全測定結果を削除

### エクスポート

測定結果を以下の形式で出力できます：

- **CSV**: 表計算ソフトで開ける形式
- **JSON**: プログラムで処理しやすい構造化形式

出力には座標、ピクセル距離、キャリブレーション済み距離が含まれます。

## インストール

```bash
# リポジトリをクローン
git clone https://github.com/higumachan/sampo.git
cd sampo

# ビルド・実行
cargo run --release
```

## 動作環境

- Windows / macOS / Linux
- Rust 2024 Edition

## 依存ライブラリ

- [egui](https://github.com/emilk/egui) - GUI フレームワーク
- [image](https://github.com/image-rs/image) - 画像処理
- [rfd](https://github.com/PolyMeilex/rfd) - ファイルダイアログ
- [arboard](https://github.com/1Password/arboard) - クリップボード操作
- [serde](https://github.com/serde-rs/serde) - シリアライズ

## ライセンス

MIT License
