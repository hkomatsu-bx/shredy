//! DCT ベース知覚ハッシュ（pHash）の計算（仕様 §3.12）。
//!
//! 画像を 32×32 グレースケールへ縮小 → 2D DCT-II → 左上 8×8 係数 → AC 平均を
//! しきい値に 64-bit へ量子化する。スケール不変（常に 32×32 へ縮小するため、
//! 解像度の異なる同一視覚パターンは同じ pHash になる）。
//!
//! 2D DCT は固定 32 点なので、`rustdct`（rustfft 依存）を持ち込まず余弦テーブルに
//! よる直接 DCT-II で実装する。pHash のビット判定は係数の相対比較のみに依存するため
//! 正規化係数は不要で、結果は数学的に等価（U10 設計判断）。

use std::io::Cursor;
use std::sync::OnceLock;

use image::imageops::FilterType;
use image::{ImageReader, Limits};

use crate::error::{CoreError, Result};

/// 縮小後の一辺（仕様 §3.12: 32×32）。
const SIZE: usize = 32;
/// pHash に使う左上ブロックの一辺（8×8 = 64 係数）。
const HASH_SIZE: usize = 8;
/// デコードを許可する画像の最大辺（px）。32×32 へ縮小するため大きな原寸は不要で、
/// デコード爆弾（小さな圧縮ファイルから巨大ピクセルバッファを展開）を防ぐ。
const MAX_DECODE_DIMENSION: u32 = 32_768;
/// デコード時の確保上限（512 MiB）。寸法制限と二重で OOM DoS を防ぐ。
const MAX_DECODE_ALLOC: u64 = 512 * 1024 * 1024;

/// 1D DCT-II の余弦テーブル `cos(π/N·(n+0.5)·k)`（初回のみ計算）。
fn cos_table() -> &'static [[f32; SIZE]; SIZE] {
    static TABLE: OnceLock<[[f32; SIZE]; SIZE]> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut t = [[0f32; SIZE]; SIZE];
        let factor = std::f32::consts::PI / SIZE as f32;
        for (k, row) in t.iter_mut().enumerate() {
            for (n, cell) in row.iter_mut().enumerate() {
                *cell = (factor * (n as f32 + 0.5) * k as f32).cos();
            }
        }
        t
    })
}

/// 画像バイト列から 64-bit pHash を計算する。デコード失敗・上限超過は `Err`（呼び出し側で除外）。
pub fn compute(bytes: &[u8]) -> Result<u64> {
    hash_dynamic(&decode_limited(bytes)?)
}

/// HEIC/HEIF バイト列から 64-bit pHash を計算する（純 Rust `heic` クレートで HEVC 復号）。
///
/// `compute` と同じ pHash 経路（[`hash_dynamic`]）を共有し、デコード方式のみ差し替える。
/// デコード失敗・上限超過は `Err`（呼び出し側で除外）。
#[cfg(feature = "heic")]
pub fn compute_heic(bytes: &[u8]) -> Result<u64> {
    hash_dynamic(&decode_heic_limited(bytes)?)
}

/// デコード済み画像から 64-bit pHash を計算する（仕様 §3.12 step 1-4）。
fn hash_dynamic(img: &image::DynamicImage) -> Result<u64> {
    // 常に 32×32 へ強制縮小（アスペクト比無視）→ L8 グレースケール（仕様 §3.12 step 1-2）。
    let gray = img
        .resize_exact(SIZE as u32, SIZE as u32, FilterType::CatmullRom)
        .to_luma8();
    let pixels = gray.as_raw();
    if pixels.len() != SIZE * SIZE {
        return Err(CoreError::Image(format!(
            "unexpected luma size: {}",
            pixels.len()
        )));
    }
    let mut matrix = [0f32; SIZE * SIZE];
    for (dst, &p) in matrix.iter_mut().zip(pixels.iter()) {
        *dst = f32::from(p);
    }
    let block = dct2d_low(&matrix);
    hash_from_coeffs(&block)
}

/// 寸法・確保量に上限を設けて画像をデコードする（デコード爆弾による OOM DoS 対策）。
fn decode_limited(bytes: &[u8]) -> Result<image::DynamicImage> {
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| CoreError::Image(format!("format: {e}")))?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_DECODE_DIMENSION);
    limits.max_image_height = Some(MAX_DECODE_DIMENSION);
    limits.max_alloc = Some(MAX_DECODE_ALLOC);
    reader.limits(limits);
    reader
        .decode()
        .map_err(|e| CoreError::Image(format!("decode: {e}")))
}

/// HEIC/HEIF を RGBA8 へデコードし `DynamicImage` を返す。`image` クレートと同等の
/// 寸法・メモリ上限を `heic::Limits` でミラーし、デコード爆弾による OOM DoS を防ぐ。
///
/// 並列化（`heic` の `parallel` feature）は使わない。画像類似度は呼び出し側で
/// ファイル単位の rayon 並列化済みのため、ここでの内部並列はネスト過多になる。
///
/// pHash 計算（[`compute_heic`]）に加え、親モジュールの [`super::decode_heic_rgba8`]
/// （GUI プレビュー等のデコード公開口）からも共有するため `pub(super)`。
#[cfg(feature = "heic")]
pub(super) fn decode_heic_limited(bytes: &[u8]) -> Result<image::DynamicImage> {
    use heic::{DecoderConfig, Limits as HeicLimits, PixelLayout};

    let mut limits = HeicLimits::default();
    limits.max_width = Some(u64::from(MAX_DECODE_DIMENSION));
    limits.max_height = Some(u64::from(MAX_DECODE_DIMENSION));
    // 出力 RGBA8（4 bytes/px）の確保上限（MAX_DECODE_ALLOC）と整合する総ピクセル数上限。
    // 辺・面積・メモリの三層でデコード爆弾/OOM を防ぐ（`image` 経路の `max_alloc` に対応）。
    limits.max_pixels = Some(MAX_DECODE_ALLOC / 4);
    limits.max_memory_bytes = Some(MAX_DECODE_ALLOC);

    let output = DecoderConfig::new()
        .decode_request(bytes)
        .with_output_layout(PixelLayout::Rgba8)
        .with_limits(&limits)
        .decode()
        .map_err(|e| CoreError::Image(format!("heic decode: {e}")))?;

    image::RgbaImage::from_raw(output.width, output.height, output.data)
        .map(image::DynamicImage::ImageRgba8)
        .ok_or_else(|| CoreError::Image("heic: pixel buffer size mismatch".into()))
}

/// 長さ 32 の 1D DCT-II のうち、低周波 `HASH_SIZE` 係数のみを計算する。
///
/// pHash は左上 `HASH_SIZE`×`HASH_SIZE` 係数しか使わないため、高周波成分は不要。
/// 出力する各係数は全係数版（k=0..SIZE）の該当要素と数学的に同一。
fn dct1d_low(input: &[f32; SIZE]) -> [f32; HASH_SIZE] {
    let table = cos_table();
    let mut out = [0f32; HASH_SIZE];
    for (k, o) in out.iter_mut().enumerate() {
        let row = &table[k];
        let mut sum = 0f32;
        for (n, &x) in input.iter().enumerate() {
            sum += x * row[n];
        }
        *o = sum;
    }
    out
}

/// 32×32 入力から左上 `HASH_SIZE`×`HASH_SIZE` の 2D DCT-II 係数だけを計算する
/// （行方向 → 列方向の 2 パス, 仕様 §3.12）。
///
/// 行・列とも低周波 `HASH_SIZE` 係数に絞ることで、全 32×32 を計算する素直版に比べ
/// 乗算数を約 1/6 に削減する。結果ブロック `block[u * HASH_SIZE + v]` は全係数版の
/// `result[u * SIZE + v]` とビット単位で等価（pHash は変わらない）。
fn dct2d_low(matrix: &[f32; SIZE * SIZE]) -> [f32; HASH_SIZE * HASH_SIZE] {
    // 行方向: 各行（全 32 行）の低周波 HASH_SIZE 係数のみを残す。
    let mut rows = [0f32; SIZE * HASH_SIZE];
    let mut buf = [0f32; SIZE];
    for r in 0..SIZE {
        buf.copy_from_slice(&matrix[r * SIZE..r * SIZE + SIZE]);
        let out = dct1d_low(&buf);
        rows[r * HASH_SIZE..r * HASH_SIZE + HASH_SIZE].copy_from_slice(&out);
    }
    // 列方向: 低周波列 v ごとに全 32 行を入力とし、低周波 HASH_SIZE 係数のみ算出。
    let mut block = [0f32; HASH_SIZE * HASH_SIZE];
    for v in 0..HASH_SIZE {
        for r in 0..SIZE {
            buf[r] = rows[r * HASH_SIZE + v];
        }
        let out = dct1d_low(&buf);
        for u in 0..HASH_SIZE {
            block[u * HASH_SIZE + v] = out[u];
        }
    }
    block
}

/// 左上 8×8 係数ブロックを、AC 成分（index 1..63）の平均をしきい値に 64-bit へ量子化する。
fn hash_from_coeffs(block: &[f32; HASH_SIZE * HASH_SIZE]) -> Result<u64> {
    // DC 成分（index 0）を除いた AC の平均（仕様 §3.12 step 5）。
    let mean = block[1..].iter().sum::<f32>() / (block.len() - 1) as f32;
    // NaN しきい値は全比較が false → hash=0 を無音で返し誤グループ化を招く。明示的に拒否する。
    if mean.is_nan() {
        return Err(CoreError::Image("non-finite DCT mean".into()));
    }
    let mut hash = 0u64;
    for (idx, &c) in block.iter().enumerate() {
        if c >= mean {
            hash |= 1u64 << idx;
        }
    }
    Ok(hash)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, ImageFormat, RgbImage};
    use std::io::Cursor;

    /// 縦半分で白黒に分かれるパターンを `w×h` で生成し PNG バイト列を返す。
    /// `invert=true` で白黒を反転する（DCT 係数が符号反転し pHash がほぼ全ビット反転）。
    fn vertical_split_png(w: u32, h: u32, invert: bool) -> Vec<u8> {
        let mut img = RgbImage::new(w, h);
        for (x, _y, px) in img.enumerate_pixels_mut() {
            let black = (x < w / 2) ^ invert;
            *px = if black {
                image::Rgb([0, 0, 0])
            } else {
                image::Rgb([255, 255, 255])
            };
        }
        encode_png(DynamicImage::ImageRgb8(img))
    }

    fn encode_png(img: DynamicImage) -> Vec<u8> {
        let mut buf = Vec::new();
        img.write_to(&mut Cursor::new(&mut buf), ImageFormat::Png)
            .unwrap();
        buf
    }

    fn hamming(a: u64, b: u64) -> u32 {
        (a ^ b).count_ones()
    }

    #[test]
    fn same_pattern_different_size_has_close_phash() {
        // スケール不変性: 同一パターンを異なる解像度で生成しても近い pHash。
        let big = compute(&vertical_split_png(128, 96, false)).unwrap();
        let small = compute(&vertical_split_png(40, 30, false)).unwrap();
        assert!(
            hamming(big, small) <= 3,
            "scale invariance: distance {} too large",
            hamming(big, small)
        );
    }

    #[test]
    fn different_patterns_have_distant_phash() {
        // 白黒反転パターンは DCT 係数が符号反転しほぼ全ビットが反転する。
        let v = compute(&vertical_split_png(64, 64, false)).unwrap();
        let inv = compute(&vertical_split_png(64, 64, true)).unwrap();
        assert!(
            hamming(v, inv) > 6,
            "distinct patterns should differ: distance {}",
            hamming(v, inv)
        );
    }

    #[test]
    fn corrupt_bytes_return_err() {
        assert!(compute(b"not an image at all").is_err());
    }

    #[test]
    fn phash_is_deterministic() {
        let png = vertical_split_png(64, 64, false);
        assert_eq!(compute(&png).unwrap(), compute(&png).unwrap());
    }

    // HEIC/HEIF デコード経路（純 Rust `heic` クレート, 仕様 §3.12）。`heic` feature 有効時のみ。
    // 入力は HEVC コードの実 HEIC サンプル（出所は tests/data/PROVENANCE.md 参照）。
    #[cfg(feature = "heic")]
    const SAMPLE_HEIC: &[u8] = include_bytes!("../../tests/data/sample.heic");

    #[cfg(feature = "heic")]
    #[test]
    fn decodes_heic_sample_to_phash() {
        // HEVC HEIC が復号でき、64bit pHash が決定的に得られる。
        let h = compute_heic(SAMPLE_HEIC).expect("HEVC HEIC sample should decode");
        assert_eq!(h, compute_heic(SAMPLE_HEIC).unwrap());
    }

    #[cfg(feature = "heic")]
    #[test]
    fn decode_heic_rgba8_returns_consistent_buffer() {
        // プレビュー用デコード公開口（親 mod）: 非空・寸法とバッファ長が RGBA8（w*h*4）で整合。
        let (w, h, rgba) =
            crate::imgsim::decode_heic_rgba8(SAMPLE_HEIC).expect("HEVC HEIC sample should decode");
        assert!(w > 0 && h > 0, "decoded dimensions must be non-zero");
        assert_eq!(
            rgba.len(),
            w as usize * h as usize * 4,
            "RGBA8 buffer length"
        );
        assert!(crate::imgsim::decode_heic_rgba8(b"not a heic").is_err());
    }

    #[cfg(feature = "heic")]
    #[test]
    fn corrupt_heic_returns_err() {
        // ftyp ヘッダは保つが中身を破壊 → デコード失敗で Err（呼び出し側で除外）。
        let mut bad = SAMPLE_HEIC[..16].to_vec();
        bad.extend_from_slice(b"\x00\x00\x00garbage payload, not valid hevc bitstream");
        assert!(compute_heic(&bad).is_err());
    }

    #[cfg(feature = "heic")]
    #[test]
    fn non_heic_bytes_return_err() {
        assert!(compute_heic(b"definitely not a heic container").is_err());
    }
}
