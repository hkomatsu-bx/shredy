//! 画像類似度の品質計測ハーネス（roadmap §1.1）。
//!
//! CC0 実写真フィクスチャ（`tests/data/imgsim/`, PROVENANCE.md）に合成変換（リサイズ /
//! JPEG 再圧縮 / クロップ / 輝度変化）を施した「同一とすべき」画像群を生成し、アルゴリズムの
//! **再現率**（同一群が 1 グループに束なるか）と**連結健全性**（別写真が鎖状連結しないか）を
//! 閾値ごとに計測する。
//!
//! 実写真を用いるのは、手続き生成パターンが DCT エネルギーを少数係数へ集中させ退化した
//! 疎 pHash になり、実写真とハッシュ分布が乖離して検証に不適だったため（roadmap §1.1）。

use std::collections::HashMap;
use std::io::Cursor;
use std::sync::mpsc::channel;
use std::time::SystemTime;

use image::{DynamicImage, ImageFormat, RgbImage};

use super::{phash, ImageSimilarityStrategy};
use crate::io::MockFs;
use crate::model::{FileEntry, FileSize};
use crate::path::LongPath;
use crate::progress::new_cancel;
use crate::strategy::GroupingStrategy;

/// CC0 実写真シード（相互に視覚的が異なる被写体, PROVENANCE.md 参照）。
const SEEDS: &[(&str, &[u8])] = &[
    (
        "ceiling",
        include_bytes!("../../tests/data/imgsim/cc0_ceiling.jpg"),
    ),
    (
        "tower",
        include_bytes!("../../tests/data/imgsim/cc0_tower.jpg"),
    ),
    (
        "sculpture",
        include_bytes!("../../tests/data/imgsim/cc0_sculpture.jpg"),
    ),
];
/// 1 シードあたりの変換数（[`transforms_of`] の生成数と一致させる）。
const TRANSFORMS_PER_SEED: usize = 7;

fn hamming(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

fn decode_seed(bytes: &[u8]) -> RgbImage {
    image::load_from_memory(bytes)
        .expect("CC0 fixture must decode")
        .to_rgb8()
}

fn to_png(img: &RgbImage) -> Vec<u8> {
    let mut buf = Vec::new();
    DynamicImage::ImageRgb8(img.clone())
        .write_to(&mut Cursor::new(&mut buf), ImageFormat::Png)
        .unwrap();
    buf
}

fn to_jpeg(img: &RgbImage, quality: u8) -> Vec<u8> {
    let mut buf = Vec::new();
    let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, quality);
    enc.encode(
        img.as_raw(),
        img.width(),
        img.height(),
        image::ExtendedColorType::Rgb8,
    )
    .unwrap();
    buf
}

/// 画像を指定辺の正方へ縮小する（ブレンド用に 2 枚の寸法を揃える）。
fn resize_to(img: &RgbImage, side: u32) -> RgbImage {
    DynamicImage::ImageRgb8(img.clone())
        .resize_exact(side, side, image::imageops::FilterType::CatmullRom)
        .to_rgb8()
}

/// 同寸の 2 画像を比率 `t`（0=a, 1=b）で画素線形補間する（鎖状連結の連鎖生成用）。
fn blend(a: &RgbImage, b: &RgbImage, t: f32) -> RgbImage {
    let mut out = RgbImage::new(a.width(), a.height());
    for (o, (pa, pb)) in out.pixels_mut().zip(a.pixels().zip(b.pixels())) {
        for c in 0..3 {
            let v = (1.0 - t) * f32::from(pa.0[c]) + t * f32::from(pb.0[c]);
            o.0[c] = v.round().clamp(0.0, 255.0) as u8;
        }
    }
    out
}

/// 復号済み実写真から「同一とすべき」変換群 `(名前, 拡張子, バイト列)` を生成する。
///
/// 想定ユースケース（roadmap §1.1）: 解像度違い（resize）・フォーマット/再圧縮違い（jpeg）・
/// 連写の微妙な構図/露出差（crop / brightness）。
fn transforms_of(seed: &RgbImage) -> Vec<(&'static str, &'static str, Vec<u8>)> {
    let dyn_seed = DynamicImage::ImageRgb8(seed.clone());
    let (w, h) = (seed.width(), seed.height());
    let (mx, my) = (w / 20, h / 20); // 5% クロップ
    let cropped = dyn_seed.crop_imm(mx, my, w - 2 * mx, h - 2 * my).to_rgb8();
    let half = dyn_seed
        .resize(w / 2, h / 2, image::imageops::FilterType::CatmullRom)
        .to_rgb8();
    let up = dyn_seed
        .resize(
            w * 3 / 2,
            h * 3 / 2,
            image::imageops::FilterType::CatmullRom,
        )
        .to_rgb8();
    let bright = dyn_seed.brighten(20).to_rgb8();
    let out = vec![
        ("orig_png", "png", to_png(seed)),
        ("jpeg_q90", "jpg", to_jpeg(seed, 90)),
        ("jpeg_q60", "jpg", to_jpeg(seed, 60)),
        ("resize_half", "png", to_png(&half)),
        ("resize_1_5x", "png", to_png(&up)),
        ("crop_5pct", "png", to_png(&cropped)),
        ("brightness", "png", to_png(&bright)),
    ];
    debug_assert_eq!(out.len(), TRANSFORMS_PER_SEED);
    out
}

/// 1 閾値ぶんの品質指標。
#[derive(Debug)]
struct Metrics {
    threshold: u8,
    total_images: usize,
    /// 各シードで「そのシードのファイルが最も多く入ったグループ」に入った件数を総数で割った平均。
    recall: f32,
    /// 最大グループサイズ（理想は 1 シードぶん＝`TRANSFORMS_PER_SEED`）。
    max_group_size: usize,
    /// 返却グループ数（理想はシード数）。
    group_count: usize,
    /// 2 種以上のシードを含むグループ数（鎖状連結の指標。0 が理想）。
    cross_seed_groups: usize,
}

fn measure(threshold: u8, seeds: &[RgbImage]) -> Metrics {
    let mut fs = MockFs::new();
    let mut entries = Vec::new();
    let mut owner: HashMap<String, usize> = HashMap::new();
    let mut per_seed_total = vec![0usize; seeds.len()];
    for (s, seed) in seeds.iter().enumerate() {
        for (name, ext, bytes) in transforms_of(seed) {
            let path = format!("C:\\seed{s}\\{name}.{ext}");
            let lp = LongPath::new(&path).unwrap();
            let len = bytes.len() as u64;
            fs.add_file(&lp, bytes);
            owner.insert(lp.as_path().to_string_lossy().to_lowercase(), s);
            entries.push(FileEntry {
                path: lp,
                size: FileSize(len),
                modified: SystemTime::UNIX_EPOCH,
            });
            per_seed_total[s] += 1;
        }
    }
    let total_images = entries.len();
    let (tx, _rx) = channel();
    let groups = ImageSimilarityStrategy::new(threshold).group(entries, &fs, &tx, &new_cancel());

    let mut max_group_size = 0;
    let mut cross_seed_groups = 0;
    let mut best = vec![0usize; seeds.len()];
    for g in &groups {
        max_group_size = max_group_size.max(g.files.len());
        let mut here = vec![0usize; seeds.len()];
        for f in &g.files {
            let key = f.path.as_path().to_string_lossy().to_lowercase();
            if let Some(&s) = owner.get(&key) {
                here[s] += 1;
            }
        }
        if here.iter().filter(|&&c| c > 0).count() >= 2 {
            cross_seed_groups += 1;
        }
        for (s, &c) in here.iter().enumerate() {
            best[s] = best[s].max(c);
        }
    }
    let recall = if seeds.is_empty() {
        0.0
    } else {
        (0..seeds.len())
            .map(|s| {
                if per_seed_total[s] == 0 {
                    0.0
                } else {
                    best[s] as f32 / per_seed_total[s] as f32
                }
            })
            .sum::<f32>()
            / seeds.len() as f32
    };
    Metrics {
        threshold,
        total_images,
        recall,
        max_group_size,
        group_count: groups.len(),
        cross_seed_groups,
    }
}

/// 実写真での品質を計測して表示する（`-- --nocapture` で確認）。改善（roadmap §1.1）の効果測定基準。
#[test]
fn imgsim_quality_measure() {
    let seeds: Vec<RgbImage> = SEEDS.iter().map(|(_, b)| decode_seed(b)).collect();
    let n = seeds.len();
    println!("\n=== 画像類似度 品質計測（実写真 CC0 × {n} + 現行 64-bit pHash）===");
    // シード間 pHash 距離（実写真は均衡ハッシュで大きく離れるはず＝退化した合成シードとの対比）。
    let seed_hashes: Vec<u64> = SEEDS
        .iter()
        .map(|(_, b)| phash::compute(b).unwrap())
        .collect();
    let mut min_seed_dist = u32::MAX;
    for a in 0..n {
        for b in (a + 1)..n {
            let d = hamming(seed_hashes[a], seed_hashes[b]);
            min_seed_dist = min_seed_dist.min(d);
            println!("  d({},{})={d}/64", SEEDS[a].0, SEEDS[b].0);
        }
    }
    println!(
        "シード間最小距離={min_seed_dist}/64。理想: group数={n} / 最大group={TRANSFORMS_PER_SEED} / 別シード混在=0"
    );
    println!(
        "{:<7} {:>7} {:>10} {:>8} {:>12}",
        "閾値%", "再現率", "最大group", "group数", "別シード混在"
    );
    for &t in &[100u8, 95, 90, 85, 80] {
        let m = measure(t, &seeds);
        println!(
            "{:<7} {:>6.0}% {:>10} {:>8} {:>12}",
            m.threshold,
            m.recall * 100.0,
            m.max_group_size,
            m.group_count,
            m.cross_seed_groups
        );
    }

    // サニティ（ハーネス自体の健全性のみ検証）。
    let m = measure(90, &seeds);
    assert_eq!(m.total_images, n * TRANSFORMS_PER_SEED);
    assert!(
        m.max_group_size >= 2,
        "同一シードの変換は 90% で少なくとも一部が束なるはず"
    );
}

/// 鎖状連結（single-linkage の過剰マージ）を再現・計測する（roadmap §1.1 の本丸）。
///
/// 別写真 A・B の間をブレンド（25/50/75%）で埋めた連鎖を作る。隣接どうしは近いが両端 A・B は
/// 遠い（＝別画像）。単連結クラスタリングは隣接の近さを推移的に伝播させ、両端まで 1 グループに
/// 束ねてしまう。これが「緩いしきい値で無関係な画像まで巨大グループ化」の正体。
/// 改善（代表クラスタリング等）を入れた際、両端が分離されるかを本テストで測る。
#[test]
fn imgsim_chaining_measure() {
    let side = 256;
    let a = resize_to(&decode_seed(SEEDS[0].1), side); // ceiling
    let b = resize_to(&decode_seed(SEEDS[2].1), side); // sculpture
    let ratios = [0.0f32, 0.25, 0.5, 0.75, 1.0];
    let imgs: Vec<RgbImage> = ratios.iter().map(|&t| blend(&a, &b, t)).collect();
    let hashes: Vec<u64> = imgs
        .iter()
        .map(|im| phash::compute(&to_png(im)).unwrap())
        .collect();

    println!("\n=== 鎖状連結の再現（A=ceiling → B=sculpture ブレンド連鎖・現行 64-bit）===");
    let adj_max = (0..hashes.len() - 1)
        .map(|i| {
            let d = hamming(hashes[i], hashes[i + 1]);
            println!(
                "  隣接 d({}%,{}%)={d}/64",
                (ratios[i] * 100.0) as u32,
                (ratios[i + 1] * 100.0) as u32
            );
            d
        })
        .max()
        .unwrap_or(0);
    let ends = hamming(hashes[0], hashes[4]);
    println!("  両端 d(A,B)={ends}/64（隣接最大={adj_max}）");
    println!(
        "{:<7} {:>8} {:>10} {:>18}",
        "閾値%", "group数", "最大group", "両端が同一group?"
    );
    for &th in &[95u8, 90, 85, 80, 75] {
        let mut fs = MockFs::new();
        let mut entries = Vec::new();
        for (i, im) in imgs.iter().enumerate() {
            let bytes = to_png(im);
            let lp = LongPath::new(format!("C:\\chain\\{i}.png")).unwrap();
            let len = bytes.len() as u64;
            fs.add_file(&lp, bytes);
            entries.push(FileEntry {
                path: lp,
                size: FileSize(len),
                modified: SystemTime::UNIX_EPOCH,
            });
        }
        let (tx, _rx) = channel();
        let groups = ImageSimilarityStrategy::new(th).group(entries, &fs, &tx, &new_cancel());
        let max_group = groups.iter().map(|g| g.files.len()).max().unwrap_or(0);
        // 両端（chain\0.png と chain\4.png）が同一グループにいるか。
        let ends_merged = groups.iter().any(|g| {
            let has0 = g
                .files
                .iter()
                .any(|f| f.path.as_path().to_string_lossy().contains("\\0.png"));
            let has4 = g
                .files
                .iter()
                .any(|f| f.path.as_path().to_string_lossy().contains("\\4.png"));
            has0 && has4
        });
        println!(
            "{:<7} {:>8} {:>10} {:>18}",
            th,
            groups.len(),
            max_group,
            ends_merged
        );
    }

    // サニティ: 連鎖が生成でき、両端は隣接より明確に遠い。
    assert!(ends > adj_max, "両端は隣接より遠いはず（連鎖の前提）");
}
