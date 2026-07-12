//! E2E 統合テスト（仕様 §4）。
//!
//! `shredy.exe` を実プロセスとして起動し、重複検索フローを検証する:
//! 走査 → 比較/グルーピング → 結果一覧表示。CLI は**重複検索専用**（削除・移動は GUI 担当）。
//!
//! 各テストは隔離された一時ディレクトリで完結する。走査対象 `data/` と設定・ログの
//! 出力先 `cfg/` を分離し、ログ書き込みが走査対象のファイル数に混入しないようにする。

use std::path::PathBuf;
use std::process::{Command, Output};

use tempfile::TempDir;

/// テスト用の隔離環境: 走査対象 `data/` と設定/ログ用 `cfg/`。
struct Sandbox {
    _tmp: TempDir,
    data: PathBuf,
    cfg: PathBuf,
}

impl Sandbox {
    fn new() -> Self {
        let tmp = TempDir::new().unwrap();
        let data = tmp.path().join("data");
        let cfg = tmp.path().join("cfg");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::create_dir_all(&cfg).unwrap();
        Self {
            _tmp: tmp,
            data,
            cfg,
        }
    }

    /// `data/` 直下にファイルを書き込む。
    fn write(&self, name: &str, content: &[u8]) {
        std::fs::write(self.data.join(name), content).expect("テストファイルの書き込みに失敗");
    }

    /// `data/` 配下の相対パス（サブフォルダ可）にファイルを書き込む。親は自動作成。
    fn write_rel(&self, rel: &str, content: &[u8]) {
        let target = self.data.join(rel);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).expect("親ディレクトリ作成に失敗");
        }
        std::fs::write(target, content).expect("テストファイルの書き込みに失敗");
    }
}

/// ビルド済み `shredy` バイナリを起動する `Command`。
fn shredy() -> Command {
    Command::new(env!("CARGO_BIN_EXE_shredy"))
}

/// 標準出力を UTF-8 文字列として取り出す。
fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn search_reports_duplicate_group() {
    // Arrange: 同一内容の 2 ファイル + 異なる 1 ファイル。
    let sb = Sandbox::new();
    sb.write("a.txt", b"duplicate content");
    sb.write("b.txt", b"duplicate content");
    sb.write("c.txt", b"unique content");

    // Act: BLAKE3 全体比較で検索（既定: 検索のみ）。
    let out = shredy()
        .args(["-S0", "--config"])
        .arg(&sb.cfg)
        .arg(&sb.data)
        .output()
        .expect("プロセス起動に失敗");

    // Assert
    assert!(out.status.success(), "終了コードが非ゼロ: {out:?}");
    let s = stdout(&out);
    assert!(s.contains("1 件の重複グループ"), "出力: {s}");
    assert!(s.contains("a.txt") || s.contains("b.txt"), "出力: {s}");
}

#[test]
fn search_reports_no_duplicates() {
    let sb = Sandbox::new();
    sb.write("a.txt", b"alpha");
    sb.write("b.txt", b"beta");

    let out = shredy()
        .args(["-S0", "--config"])
        .arg(&sb.cfg)
        .arg(&sb.data)
        .output()
        .expect("プロセス起動に失敗");

    assert!(out.status.success());
    assert!(stdout(&out).contains("重複は見つかりませんでした"));
}

#[test]
fn missing_folder_is_error() {
    let sb = Sandbox::new();
    // フォルダ引数なし。--config のみ。
    let out = shredy()
        .args(["-S0", "--config"])
        .arg(&sb.cfg)
        .output()
        .expect("プロセス起動に失敗");

    assert!(!out.status.success(), "フォルダ未指定は失敗するはず");
}

#[test]
fn image_similarity_detects_rescaled_duplicates() {
    // -S8 (画像類似度, U10): 同一視覚パターンを異なる解像度で生成 → pHash で束なる。
    let sb = Sandbox::new();
    sb.write("big.png", &vertical_split_png(120, 90, false));
    sb.write("small.png", &vertical_split_png(48, 36, false));
    sb.write("other.png", &vertical_split_png(64, 64, true)); // 白黒反転 → 別グループ

    let out = shredy()
        .args(["-S8", "--config"])
        .arg(&sb.cfg)
        .arg(&sb.data)
        .output()
        .expect("プロセス起動に失敗");

    assert!(out.status.success(), "終了コードが非ゼロ: {out:?}");
    let s = stdout(&out);
    assert!(s.contains("1 件の重複グループ"), "出力: {s}");
    assert!(s.contains("phash:"), "出力に代表ハッシュが無い: {s}");
}

/// 縦半分で白黒に分かれるパターンの PNG を生成する。`invert=true` で白黒反転。
fn vertical_split_png(w: u32, h: u32, invert: bool) -> Vec<u8> {
    use std::io::Cursor;
    let mut img = image::RgbImage::new(w, h);
    for (x, _y, px) in img.enumerate_pixels_mut() {
        let black = (x < w / 2) ^ invert;
        *px = if black {
            image::Rgb([0, 0, 0])
        } else {
            image::Rgb([255, 255, 255])
        };
    }
    let mut buf = Vec::new();
    image::DynamicImage::ImageRgb8(img)
        .write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Png)
        .expect("PNG エンコードに失敗");
    buf
}

#[test]
fn search_recurses_into_subfolders() {
    // Arrange: 別々のサブフォルダに同一内容ファイルを配置（再帰走査の検証）。
    let sb = Sandbox::new();
    sb.write_rel("sub1/a.txt", b"recursive duplicate");
    sb.write_rel("sub2/nested/b.txt", b"recursive duplicate");
    sb.write_rel("sub1/unique.txt", b"only here");

    let out = shredy()
        .args(["-S0", "--config"])
        .arg(&sb.cfg)
        .arg(&sb.data)
        .output()
        .expect("プロセス起動に失敗");

    // Assert: サブフォルダ跨ぎでも 1 グループとして検出される。
    assert!(out.status.success(), "{out:?}");
    let s = stdout(&out);
    assert!(s.contains("1 件の重複グループ"), "出力: {s}");
    assert!(s.contains("a.txt") || s.contains("b.txt"), "出力: {s}");
}

#[test]
fn all_hash_methods_detect_same_duplicate() {
    // 比較方式 0/1/2（BLAKE3 / SHA-256 / XXH3）いずれでも同一重複を検出する。
    for method in ["-S0", "-S1", "-S2"] {
        let sb = Sandbox::new();
        sb.write("a.txt", b"method coverage payload");
        sb.write("b.txt", b"method coverage payload");
        sb.write("c.txt", b"different");

        let out = shredy()
            .args([method, "--config"])
            .arg(&sb.cfg)
            .arg(&sb.data)
            .output()
            .expect("プロセス起動に失敗");

        assert!(out.status.success(), "{method} で失敗: {out:?}");
        assert!(
            stdout(&out).contains("1 件の重複グループ"),
            "{method} で重複を検出できなかった: {}",
            stdout(&out)
        );
    }
}
