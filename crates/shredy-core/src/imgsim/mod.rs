//! 画像類似度比較（S8, 仕様 §3.12）。
//!
//! DCT ベース知覚ハッシュ（pHash）を並列計算し、代表（leader）クラスタリングで束ねる
//! （各点を半径 `maxBits` 内の最近傍代表へ割り当て、無ければ新規代表にする）。リサイズ・再圧縮・
//! 連写などバイナリは異なるが視覚的に同一/近似な画像を検出する。段階的な
//! 類似率は返さず、しきい値内のペアを同一グループとする。

mod bktree;
mod phash;

/// 品質計測ハーネス（roadmap §1.1・CC0 実写真＋合成変換で再現率/連結健全性を計測）。
#[cfg(test)]
mod quality_tests;

use std::collections::HashMap;

use rayon::prelude::*;

use crate::error::{CoreError, Result};
use crate::io::FileReader;
use crate::model::{DuplicateGroup, FileEntry};
#[cfg(test)]
use crate::path::LongPath;
use crate::progress::{Cancel, Phase, ProgressEvent, ProgressTx, is_cancelled};
use crate::strategy::GroupingStrategy;

use bktree::BkTree;

/// 1 画像の読み込み上限（256 MiB）。超過は除外しメモリを保護する。
pub const IMAGE_READ_CAP: u64 = 256 * 1024 * 1024;

/// HEIC/HEIF を RGBA8 へデコードし `(幅, 高さ, RGBA8 バイト列)` を返す（プレビュー用途の公開口）。
///
/// pHash 計算（`phash::compute_heic`）と同じ [`phash::decode_heic_limited`]（デコード爆弾対策込み）
/// を共有し、デコード結果を RGBA8 で取り出す。S8 比較とは用途が異なる（描画向け）ため
/// ハッシュではなくピクセルバッファを返す。バッファ長 `w*h*4` は core 側
/// `RgbaImage::from_raw` の不変条件で保証される（呼び出し側の長さ再検証は不要）。
///
/// デコード失敗・上限超過は `Err`（呼び出し側で表示をフォールバック）。
#[cfg(feature = "heic")]
pub fn decode_heic_rgba8(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>)> {
    let rgba = phash::decode_heic_limited(bytes)?.into_rgba8();
    Ok((rgba.width(), rgba.height(), rgba.into_raw()))
}

/// 対応する画像拡張子（小文字, 仕様 §3.12）。`heic` feature 有効時のみ HEIC/HEIF を含む
/// （純 Rust `heic` クレートで HEVC 復号。feature を外すと走査時にスキップされる）。
#[cfg(feature = "heic")]
const SUPPORTED_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "bmp", "tif", "tiff", "gif", "webp", "heic", "heif",
];
#[cfg(not(feature = "heic"))]
const SUPPORTED_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "bmp", "tif", "tiff", "gif", "webp"];

/// 画像類似度（S8）。pHash のハミング距離クラスタリングで束ねる。
pub struct ImageSimilarityStrategy {
    threshold: u8,
}

impl ImageSimilarityStrategy {
    /// 類似度しきい値（0〜100%）を指定して戦略を作る。
    #[must_use]
    pub fn new(threshold: u8) -> Self {
        Self { threshold }
    }
}

impl GroupingStrategy for ImageSimilarityStrategy {
    fn label(&self) -> &'static str {
        "画像類似度"
    }

    fn group(
        &self,
        entries: Vec<FileEntry>,
        fs: &dyn FileReader,
        progress: &ProgressTx,
        cancel: &Cancel,
    ) -> Vec<DuplicateGroup> {
        let _ = progress.send(ProgressEvent::Phase(Phase::Comparing));
        let items = compute_hashes(&entries, fs, cancel);
        if items.len() < 2 {
            return Vec::new();
        }
        let assign = cluster(&items, max_bits(self.threshold), cancel);
        to_groups(&items, &assign)
    }
}

/// フェーズ 1-2: 全画像の pHash を並列計算する。デコード失敗・非対応形式は除外する。
fn compute_hashes(
    entries: &[FileEntry],
    fs: &dyn FileReader,
    cancel: &Cancel,
) -> Vec<(FileEntry, u64)> {
    entries
        .par_iter()
        .filter_map(|e| {
            if is_cancelled(cancel) {
                return None;
            }
            let hash = fingerprint(e, fs).ok()?;
            Some((e.clone(), hash))
        })
        .collect()
}

/// フェーズ 3: 代表（leader）クラスタリング。各点を「既存クラスタの代表から半径 `radius`
/// 内で最も近い代表」のクラスタへ割り当て、無ければ自身を代表とする新規クラスタを作る。
///
/// 単連結（Union-Find）は中間画像を介して別画像を推移的にマージし、緩いしきい値で巨大な
/// 無意味グループ（鎖状連結）を生む（roadmap §1.1 の課題）。代表クラスタリングは所属を
/// 「固定された代表への距離」で決めるため鎖状連結が構造的に起きない。代表は各クラスタの
/// 最初の一件で固定する。代表集合を BK-tree で保持し、半径内の候補を平均 O(N log N) で引く。
///
/// 戻り値 `assign[i]` は点 i のクラスタ id。中断時は未処理点を各々独立クラスタにする
/// （ベストエフォート。`to_groups` が 2 件未満を除外するため単独点は結果に出ない）。
fn cluster(items: &[(FileEntry, u64)], radius: u32, cancel: &Cancel) -> Vec<usize> {
    let mut reps = BkTree::new();
    let mut rep_hashes: Vec<u64> = Vec::new();
    let mut assign = vec![usize::MAX; items.len()];
    let mut cand = Vec::new();
    for (i, (_, hash)) in items.iter().enumerate() {
        if is_cancelled(cancel) {
            break;
        }
        cand.clear();
        // 代表 BK-tree の payload はクラスタ id。半径内の代表候補を集める。
        reps.search(*hash, radius, &mut cand);
        match cand
            .iter()
            .copied()
            .min_by_key(|&id| (*hash ^ rep_hashes[id]).count_ones())
        {
            // 半径内に代表があれば最近傍代表のクラスタへ加わる（代表は更新しない）。
            Some(best) => assign[i] = best,
            // 無ければ自身を代表とする新規クラスタを作る。
            None => {
                let id = rep_hashes.len();
                rep_hashes.push(*hash);
                reps.insert(*hash, id);
                assign[i] = id;
            }
        }
    }
    // 中断で未割当（usize::MAX）の点は各々独立クラスタにする（結果に出ない単独点）。
    let mut fresh = rep_hashes.len();
    for slot in assign.iter_mut() {
        if *slot == usize::MAX {
            *slot = fresh;
            fresh += 1;
        }
    }
    assign
}

/// フェーズ 4-5: クラスタ割当を集約し 2 件以上を `DuplicateGroup` として返す。
fn to_groups(items: &[(FileEntry, u64)], assign: &[usize]) -> Vec<DuplicateGroup> {
    let mut components: HashMap<usize, Vec<usize>> = HashMap::with_capacity(items.len());
    for (i, &id) in assign.iter().enumerate() {
        components.entry(id).or_default().push(i);
    }
    components
        .into_values()
        .filter(|members| members.len() >= 2)
        .map(|mut members| {
            members.sort_unstable(); // 入力順で先頭を決定的にする。
            let first_hash = items[members[0]].1;
            let files = members.iter().map(|&m| items[m].0.clone()).collect();
            DuplicateGroup {
                representative: format!("phash:{first_hash:016x}"),
                files,
            }
        })
        .collect()
}

/// 類似度しきい値（%）を許容ハミングビット数へ変換する（仕様 §3.12）。
///
/// `maxBits = round((100 - threshold) * 64 / 100)`。100→0, 95→3, 90→6, 0→64。
#[must_use]
fn max_bits(threshold: u8) -> u32 {
    let t = u32::from(threshold.min(100));
    ((100 - t) * 64 + 50) / 100
}

/// 1 画像から pHash を計算する。非対応拡張子・解析失敗は `Err`。
fn fingerprint(entry: &FileEntry, fs: &dyn FileReader) -> Result<u64> {
    let ext = entry
        .path
        .extension_lower()
        .ok_or_else(|| CoreError::Image("no extension".into()))?;
    if !SUPPORTED_EXTENSIONS.contains(&ext.as_str()) {
        return Err(CoreError::Image(format!("unsupported extension: {ext}")));
    }
    let bytes = crate::io::read_all_capped(&entry.path, fs, IMAGE_READ_CAP)?;
    // HEIC/HEIF は `image` クレートが非対応のため純 Rust `heic` クレートで復号する。
    // 拡張子は SUPPORTED_EXTENSIONS のゲートを通っており、feature 有効時のみここへ到達する。
    #[cfg(feature = "heic")]
    if matches!(ext.as_str(), "heic" | "heif") {
        return phash::compute_heic(&bytes);
    }
    phash::compute(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::MockFs;
    use crate::model::FileSize;
    use crate::progress::new_cancel;
    use image::{DynamicImage, ImageFormat, RgbImage};
    use std::io::Cursor;
    use std::sync::mpsc::channel;
    use std::time::SystemTime;

    /// 縦半分で白黒に分かれる PNG。`invert=true` で白黒反転（pHash がほぼ全ビット反転）。
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
        let mut buf = Vec::new();
        DynamicImage::ImageRgb8(img)
            .write_to(&mut Cursor::new(&mut buf), ImageFormat::Png)
            .unwrap();
        buf
    }

    fn entry(fs: &mut MockFs, path: &str, content: Vec<u8>) -> FileEntry {
        let lp = LongPath::new(path).unwrap();
        let len = content.len() as u64;
        fs.add_file(&lp, content);
        FileEntry {
            path: lp,
            size: FileSize(len),
            modified: SystemTime::UNIX_EPOCH,
        }
    }

    fn run(
        strategy: &dyn GroupingStrategy,
        fs: &MockFs,
        entries: Vec<FileEntry>,
    ) -> Vec<DuplicateGroup> {
        let (tx, _rx) = channel();
        strategy.group(entries, fs, &tx, &new_cancel())
    }

    #[test]
    fn max_bits_matches_spec_table() {
        assert_eq!(max_bits(100), 0);
        assert_eq!(max_bits(95), 3);
        assert_eq!(max_bits(90), 6);
        assert_eq!(max_bits(0), 64);
        // 範囲外は 100 にクランプ。
        assert_eq!(max_bits(200), 0);
    }

    #[test]
    fn rescaled_images_group_together() {
        let mut fs = MockFs::new();
        // 同一パターンの異サイズ + 反転（無関係）パターン。
        let a = entry(&mut fs, "C:\\a.png", vertical_split_png(120, 90, false));
        let b = entry(&mut fs, "C:\\b.png", vertical_split_png(48, 36, false));
        let c = entry(&mut fs, "C:\\c.png", vertical_split_png(64, 64, true));
        let s = ImageSimilarityStrategy::new(90);
        let groups = run(&s, &fs, vec![a, b, c]);
        assert_eq!(groups.len(), 1, "縦割りパターン 2 枚のみ束なる");
        assert_eq!(groups[0].files.len(), 2);
        assert!(groups[0].representative.starts_with("phash:"));
    }

    #[test]
    fn threshold_100_requires_exact_phash() {
        let mut fs = MockFs::new();
        // 同一パターンを同サイズで 2 枚 → pHash 完全一致で束なる。
        let a = entry(&mut fs, "C:\\a.png", vertical_split_png(64, 64, false));
        let b = entry(&mut fs, "C:\\b.png", vertical_split_png(64, 64, false));
        let s = ImageSimilarityStrategy::new(100);
        let groups = run(&s, &fs, vec![a, b]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].files.len(), 2);
    }

    #[test]
    fn non_image_and_corrupt_files_are_skipped() {
        let mut fs = MockFs::new();
        let a = entry(&mut fs, "C:\\a.txt", b"hello".to_vec());
        let b = entry(&mut fs, "C:\\b.png", b"not really a png".to_vec());
        let s = ImageSimilarityStrategy::new(90);
        assert!(run(&s, &fs, vec![a, b]).is_empty());
    }

    #[test]
    fn fewer_than_two_images_yields_no_group() {
        let mut fs = MockFs::new();
        let a = entry(&mut fs, "C:\\a.png", vertical_split_png(64, 64, false));
        let s = ImageSimilarityStrategy::new(90);
        assert!(run(&s, &fs, vec![a]).is_empty());
    }

    #[test]
    fn make_strategy_now_supports_image() {
        let cfg = crate::config::ScanConfigBuilder::new().build();
        assert!(
            crate::strategy::make_strategy(
                crate::config::CompareMethod::ImageSimilarity,
                &cfg,
                None
            )
            .is_some()
        );
    }
}

#[cfg(test)]
mod prop_tests {
    use super::*;
    use crate::io::MockFs;
    use crate::model::FileSize;
    use crate::progress::new_cancel;
    use image::{DynamicImage, ImageFormat, RgbImage};
    use proptest::prelude::*;
    use std::collections::BTreeSet;
    use std::io::Cursor;
    use std::sync::mpsc::channel;
    use std::time::SystemTime;

    /// 縦割りパターン（`invert` で白黒反転）を `64+delta` 角の正方 PNG で生成する。
    /// サイズ差はスケール不変性によって pHash に影響しない。
    fn pattern_png(invert: bool, delta: u32) -> Vec<u8> {
        let side = 48 + delta;
        let mut img = RgbImage::new(side, side);
        for (x, _y, px) in img.enumerate_pixels_mut() {
            let black = (x < side / 2) ^ invert;
            *px = if black {
                image::Rgb([0, 0, 0])
            } else {
                image::Rgb([255, 255, 255])
            };
        }
        let mut buf = Vec::new();
        DynamicImage::ImageRgb8(img)
            .write_to(&mut Cursor::new(&mut buf), ImageFormat::Png)
            .unwrap();
        buf
    }

    proptest! {
        // PBT-U10: 縦割り（通常 / 反転）2 種をランダムなサイズで混在させても、
        // グルーピング結果はパターン識別子ごとの集合（2 件以上）と一致する。
        // 反転は pHash がほぼ全ビット反転するため両者は決して同一グループにならない。
        #[test]
        fn images_group_by_visual_pattern(
            flags in proptest::collection::vec(any::<bool>(), 0..6),
            deltas in proptest::collection::vec(0u32..20, 0..6),
        ) {
            let mut fs = MockFs::new();
            let entries: Vec<FileEntry> = flags
                .iter()
                .enumerate()
                .map(|(i, &inv)| {
                    let delta = deltas.get(i).copied().unwrap_or(0);
                    let lp = LongPath::new(format!("C:\\img{i}.png")).unwrap();
                    fs.add_file(&lp, pattern_png(inv, delta));
                    FileEntry { path: lp, size: FileSize(1), modified: SystemTime::UNIX_EPOCH }
                })
                .collect();

            // オラクル: パターン（通常/反転）ごとにまとめ 2 件以上のみ。
            let mut by_pattern: std::collections::HashMap<bool, BTreeSet<String>> =
                std::collections::HashMap::new();
            for (i, &inv) in flags.iter().enumerate() {
                by_pattern.entry(inv).or_default().insert(format!("c:\\img{i}.png"));
            }
            let oracle: BTreeSet<BTreeSet<String>> =
                by_pattern.into_values().filter(|s| s.len() >= 2).collect();

            let s = ImageSimilarityStrategy::new(90);
            let (tx, _rx) = channel();
            let groups = s.group(entries, &fs, &tx, &new_cancel());
            let got: BTreeSet<BTreeSet<String>> = groups
                .into_iter()
                .map(|g| g.files.iter()
                    .map(|f| f.path.as_path().to_string_lossy().to_lowercase())
                    .collect())
                .collect();
            prop_assert_eq!(got, oracle);
        }
    }
}
