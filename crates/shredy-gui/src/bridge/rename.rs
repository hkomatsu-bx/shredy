//! 一括リネーム（§3.10.1）の GUI ブリッジ。
//!
//! モック準拠の **11 方式**（連番の位置・大文字小文字の対象/変換はパラメータ）と、選択中
//! ステップのインライン編集を支える純関数群:
//!
//! - [`RenameForm`] — 編集フォームの汎用値（方式 index＋テキスト 2＋数値 2＋選択 4）
//! - [`make_step`] — フォーム値 → core [`RenameStep`]（空欄は安全な既定へ倒す）
//! - [`decompose`] — [`RenameStep`] → フォーム値（行選択時にフォームへ展開）
//! - [`default_form`] — 方式の既定パラメータ（モック `DEFAULT_PARAMS` 準拠）
//! - [`method_name`] / [`step_summary`] — ステップ行の方式名と設定要約

use shredy_core::{CaseConvert, CaseTarget, ExifDateFormat, RenameStep, SeqPosition};

/// 方式 index（モックのカテゴリ順）: 追加 0-3 / 変換 4 / 削除 5-6 / 置換 7-8 / その他 9-10。
pub const METHOD_COUNT: i32 = 11;

/// 桁数の上限（連番のゼロ詰め。非現実的な巨大値を弾く）。
const MAX_DIGITS: i32 = 18;

/// EXIF 命名パターンが空のときの既定（モック準拠）。
pub const DEFAULT_EXIF_PATTERN: &str = "{撮影日時}";

/// 編集フォームの汎用値。Slint の `edit_*` プロパティ群と 1:1 対応する。
///
/// `t1`/`t2` はテキスト、`n1`/`n2` は数値（方式により意味が変わる）、`fmt`=EXIF 日時書式、
/// `pos`=連番位置（0=先頭/1=末尾）、`target`=大小対象（0=ファイル名/1=拡張子）、
/// `conv`=大小変換（0=大文字/1=小文字）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenameForm {
    pub method: i32,
    pub t1: String,
    pub t2: String,
    pub n1: i32,
    pub n2: i32,
    pub fmt: i32,
    pub pos: i32,
    pub target: i32,
    pub conv: i32,
}

impl Default for RenameForm {
    fn default() -> Self {
        default_form(0)
    }
}

/// 方式の既定パラメータ（モック `DEFAULT_PARAMS` 準拠・シナリオ固有値は汎用化）。
#[must_use]
pub fn default_form(method: i32) -> RenameForm {
    let mut f = RenameForm {
        method,
        t1: String::new(),
        t2: String::new(),
        n1: 3,
        n2: 1,
        fmt: 0,
        pos: 1,    // 末尾
        target: 1, // 拡張子
        conv: 1,   // 小文字
    };
    match method {
        0 => f.t1 = "_".into(),                  // 連番: t1=区切り
        1 | 2 => f.t2 = "_".into(),              // 文字列/日時＋連番: t2=区切り
        3 => f.t1 = DEFAULT_EXIF_PATTERN.into(), // EXIF: t1=パターン
        6 => {
            f.n1 = 1; // 位置指定で削除: 開始位置
            f.n2 = 1; // 削除文字数
        }
        _ => {}
    }
    f
}

/// EXIF 日時書式 index → [`ExifDateFormat`]（0=日時 / 1=日付ダッシュ / 2=日付）。
fn exif_fmt(i: i32) -> ExifDateFormat {
    match i {
        1 => ExifDateFormat::DateDash,
        2 => ExifDateFormat::DateCompact,
        _ => ExifDateFormat::DateTimeCompact,
    }
}

fn exif_fmt_index(f: ExifDateFormat) -> i32 {
    match f {
        ExifDateFormat::DateTimeCompact => 0,
        ExifDateFormat::DateDash => 1,
        ExifDateFormat::DateCompact => 2,
    }
}

/// 今日の日付（ローカル時刻）を `YYYYMMDD` で返す（日時＋連番の既定値）。
#[must_use]
pub fn today_yyyymmdd() -> String {
    chrono::Local::now().format("%Y%m%d").to_string()
}

/// 実行時既定を解決したステップ列を返す（プレビュー/適用の直前に呼ぶ）。
///
/// 日時＋連番の日付空欄＝今日の日付（「日付が入らない」事故防止）、EXIF 命名のパターン
/// 空欄＝[`DEFAULT_EXIF_PATTERN`]（モック準拠）。編集中のフォーム値は生のまま保持される。
#[must_use]
pub fn resolve_runtime_defaults(steps: &[shredy_core::RecipeStep]) -> Vec<shredy_core::RecipeStep> {
    steps
        .iter()
        .map(|s| {
            let mut s = s.clone();
            match &mut s.step {
                RenameStep::DateSequence { date, .. } if date.trim().is_empty() => {
                    *date = today_yyyymmdd();
                }
                RenameStep::ExifName { pattern, .. } if pattern.trim().is_empty() => {
                    *pattern = DEFAULT_EXIF_PATTERN.to_string();
                }
                _ => {}
            }
            s
        })
        .collect()
}

/// フォーム値からリネーム方式を組み立てる（方式 index 範囲外のみ `None`）。
///
/// インライン編集では入力途中の空欄が正常系のため、テキスト空でもステップは成立させる
/// （core 側が空テキストを無変更として扱う）。日時空欄=今日 / EXIF パターン空欄=既定。
///
/// [`decompose`] の逆変換。方式 index ＝ 1 アームで写す純粋なデータ変換のため、網羅 match の
/// まま単一関数に保つ（分割は凝集度を下げるだけ）。
#[must_use]
pub fn make_step(f: &RenameForm) -> Option<RenameStep> {
    let digits = f.n1.clamp(0, MAX_DIGITS) as u8;
    let start = f.n2.max(0) as u64;
    match f.method {
        0 => Some(RenameStep::Sequence {
            position: if f.pos == 0 {
                SeqPosition::Prefix
            } else {
                SeqPosition::Suffix
            },
            digits,
            start,
            sep: f.t1.clone(),
        }),
        1 => Some(RenameStep::StringSequence {
            text: f.t1.clone(),
            digits,
            start,
            sep: f.t2.clone(),
        }),
        // 日時・EXIF パターンの空欄はここでは解決しない（インライン編集で全消し→即復元
        // されると打ち直せないため）。実行直前に [`resolve_runtime_defaults`] で解決する。
        2 => Some(RenameStep::DateSequence {
            date: f.t1.clone(),
            digits,
            start,
            sep: f.t2.clone(),
        }),
        3 => Some(RenameStep::ExifName {
            pattern: f.t1.clone(),
            date_format: exif_fmt(f.fmt),
            digits,
            start,
        }),
        4 => Some(RenameStep::ChangeCase {
            target: if f.target == 1 {
                CaseTarget::Ext
            } else {
                CaseTarget::Stem
            },
            convert: if f.conv == 0 {
                CaseConvert::Upper
            } else {
                CaseConvert::Lower
            },
        }),
        5 => Some(RenameStep::RemoveString { text: f.t1.clone() }),
        6 => Some(RenameStep::RemoveRange {
            start: f.n1.max(0) as usize,
            count: f.n2.max(0) as usize,
        }),
        7 => Some(RenameStep::ReplaceText {
            from: f.t1.clone(),
            to: f.t2.clone(),
        }),
        8 => Some(RenameStep::ReplaceRegex {
            pattern: f.t1.clone(),
            replace: f.t2.clone(),
        }),
        9 => Some(RenameStep::UrlDecode),
        10 => Some(RenameStep::CombineDiacritics),
        _ => None,
    }
}

/// `u64`/`usize` をフォーム用 `i32` へ飽和変換する（範囲外は `i32::MAX`）。
///
/// 連番開始値・桁数・文字位置などは現実的な範囲に収まるため上限飽和で十分。
fn to_form_i32<T: TryInto<i32>>(v: T) -> i32 {
    v.try_into().unwrap_or(i32::MAX)
}

/// リネーム方式をフォーム値へ分解する（行選択時にフォームへ展開する逆変換）。
///
/// 11 バリアントを 1 アーム＝1 方式で写す純粋なデータ変換のため、網羅 match のまま
/// 単一関数に保つ（分割は凝集度を下げるだけ。各アームは独立で副作用なし）。
#[must_use]
pub fn decompose(step: &RenameStep) -> RenameForm {
    match step {
        RenameStep::Sequence {
            position,
            digits,
            start,
            sep,
        } => RenameForm {
            t1: sep.clone(),
            n1: i32::from(*digits),
            n2: to_form_i32(*start),
            pos: if matches!(position, SeqPosition::Prefix) {
                0
            } else {
                1
            },
            ..default_form(0)
        },
        RenameStep::StringSequence {
            text,
            digits,
            start,
            sep,
        } => RenameForm {
            t1: text.clone(),
            t2: sep.clone(),
            n1: i32::from(*digits),
            n2: to_form_i32(*start),
            ..default_form(1)
        },
        RenameStep::DateSequence {
            date,
            digits,
            start,
            sep,
        } => RenameForm {
            t1: date.clone(),
            t2: sep.clone(),
            n1: i32::from(*digits),
            n2: to_form_i32(*start),
            ..default_form(2)
        },
        RenameStep::ExifName {
            pattern,
            date_format,
            digits,
            start,
        } => RenameForm {
            t1: pattern.clone(),
            n1: i32::from(*digits),
            n2: to_form_i32(*start),
            fmt: exif_fmt_index(*date_format),
            ..default_form(3)
        },
        RenameStep::ChangeCase { target, convert } => RenameForm {
            target: if matches!(target, CaseTarget::Ext) {
                1
            } else {
                0
            },
            conv: if matches!(convert, CaseConvert::Upper) {
                0
            } else {
                1
            },
            ..default_form(4)
        },
        RenameStep::RemoveString { text } => RenameForm {
            t1: text.clone(),
            ..default_form(5)
        },
        RenameStep::RemoveRange { start, count } => RenameForm {
            n1: to_form_i32(*start),
            n2: to_form_i32(*count),
            ..default_form(6)
        },
        RenameStep::ReplaceText { from, to } => RenameForm {
            t1: from.clone(),
            t2: to.clone(),
            ..default_form(7)
        },
        RenameStep::ReplaceRegex { pattern, replace } => RenameForm {
            t1: pattern.clone(),
            t2: replace.clone(),
            ..default_form(8)
        },
        RenameStep::UrlDecode => default_form(9),
        RenameStep::CombineDiacritics => default_form(10),
    }
}

/// 方式名（ステップ行の主表示・モックの方式タブ名と一致）。
#[must_use]
pub fn method_name(step: &RenameStep) -> &'static str {
    match step {
        RenameStep::Sequence { .. } => "連番",
        RenameStep::StringSequence { .. } => "文字列＋連番",
        RenameStep::DateSequence { .. } => "日時＋連番",
        RenameStep::ExifName { .. } => "EXIF情報",
        RenameStep::ChangeCase { .. } => "大文字・小文字",
        RenameStep::RemoveString { .. } => "文字列を削除",
        RenameStep::RemoveRange { .. } => "位置指定で削除",
        RenameStep::ReplaceText { .. } => "テキスト置換",
        RenameStep::ReplaceRegex { .. } => "正規表現置換",
        RenameStep::UrlDecode => "URLデコード",
        RenameStep::CombineDiacritics => "濁点・半濁点の結合",
    }
}

/// 設定内容の一言要約（ステップ行の副表示・モック `stepSummary` 準拠）。
#[must_use]
pub fn step_summary(step: &RenameStep) -> String {
    match step {
        RenameStep::Sequence {
            position,
            digits,
            start,
            ..
        } => {
            let pos = match position {
                SeqPosition::Prefix => "先頭",
                SeqPosition::Suffix => "末尾",
            };
            format!("{pos}に {digits}桁（{start}〜）")
        }
        RenameStep::StringSequence { text, digits, .. } => {
            format!("「{text}」＋連番 {digits}桁")
        }
        RenameStep::DateSequence { date, digits, .. } => format!("{date} ＋連番 {digits}桁"),
        RenameStep::ExifName { pattern, .. } => pattern.clone(),
        RenameStep::ChangeCase { target, convert } => {
            let t = match target {
                CaseTarget::Stem => "ファイル名",
                CaseTarget::Ext => "拡張子",
            };
            let c = match convert {
                CaseConvert::Upper => "大文字",
                CaseConvert::Lower => "小文字",
            };
            format!("{t}を{c}に")
        }
        RenameStep::RemoveString { text } => format!("「{text}」を削除"),
        RenameStep::RemoveRange { start, count } => format!("{start}文字目から{count}文字"),
        RenameStep::ReplaceText { from, to } => format!("「{from}」→「{to}」"),
        RenameStep::ReplaceRegex { pattern, replace } => format!("/{pattern}/ →「{replace}」"),
        RenameStep::UrlDecode => "%XX をデコード".to_string(),
        RenameStep::CombineDiacritics => "ハ\u{309c} → パ に結合".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn make_step_maps_sequence_position_param() {
        let mut f = default_form(0);
        f.pos = 0;
        assert!(matches!(
            make_step(&f),
            Some(RenameStep::Sequence {
                position: SeqPosition::Prefix,
                ..
            })
        ));
        f.pos = 1;
        assert!(matches!(
            make_step(&f),
            Some(RenameStep::Sequence {
                position: SeqPosition::Suffix,
                ..
            })
        ));
    }

    #[test]
    fn make_step_maps_change_case_params() {
        let mut f = default_form(4);
        f.target = 0;
        f.conv = 0;
        assert!(matches!(
            make_step(&f),
            Some(RenameStep::ChangeCase {
                target: CaseTarget::Stem,
                convert: CaseConvert::Upper,
            })
        ));
        f.target = 1;
        f.conv = 1;
        assert!(matches!(
            make_step(&f),
            Some(RenameStep::ChangeCase {
                target: CaseTarget::Ext,
                convert: CaseConvert::Lower,
            })
        ));
    }

    #[test]
    fn make_step_keeps_raw_empty_values_for_inline_editing() {
        // 編集中は空欄を生のまま保持する（全消し→即復元では打ち直せない）。
        let mut f = default_form(2);
        f.t1 = String::new();
        assert!(
            matches!(make_step(&f), Some(RenameStep::DateSequence { ref date, .. }) if date.is_empty())
        );
        let mut f = default_form(3);
        f.t1 = String::new();
        assert!(
            matches!(make_step(&f), Some(RenameStep::ExifName { ref pattern, .. }) if pattern.is_empty())
        );
    }

    #[test]
    fn resolve_runtime_defaults_fills_date_and_exif_pattern() {
        use shredy_core::RecipeStep;
        let steps = vec![
            RecipeStep::new(RenameStep::DateSequence {
                date: " ".into(),
                digits: 2,
                start: 1,
                sep: "_".into(),
            }),
            RecipeStep::new(RenameStep::ExifName {
                pattern: String::new(),
                date_format: ExifDateFormat::DateCompact,
                digits: 3,
                start: 1,
            }),
            // 明示値は変更しない。
            RecipeStep::new(RenameStep::DateSequence {
                date: "20240309".into(),
                digits: 2,
                start: 1,
                sep: "_".into(),
            }),
        ];
        let resolved = resolve_runtime_defaults(&steps);
        assert!(
            matches!(&resolved[0].step, RenameStep::DateSequence { date, .. } if *date == today_yyyymmdd())
        );
        assert!(
            matches!(&resolved[1].step, RenameStep::ExifName { pattern, .. } if pattern == DEFAULT_EXIF_PATTERN)
        );
        assert!(
            matches!(&resolved[2].step, RenameStep::DateSequence { date, .. } if date == "20240309")
        );
    }

    #[test]
    fn empty_text_steps_are_still_created() {
        // インライン編集では空欄が正常系（core が無変更として扱う）。
        for m in [5, 7, 8] {
            let f = default_form(m);
            assert!(make_step(&f).is_some(), "method {m} は空欄でも成立する");
        }
        assert!(make_step(&RenameForm {
            method: 99,
            ..default_form(0)
        })
        .is_none());
    }

    #[test]
    fn decompose_roundtrips_every_method() {
        // 全方式で make→decompose→make が一致する（フォーム⇔ステップの整合）。
        for m in 0..METHOD_COUNT {
            let mut f = default_form(m);
            f.t1 = match m {
                0 => "-".into(),
                2 => "20240309".into(),
                3 => "{機種}_{連番}".into(),
                5 | 7 | 8 => "abc".into(),
                _ => f.t1,
            };
            let step = make_step(&f).unwrap();
            let back = decompose(&step);
            let step2 = make_step(&back).unwrap();
            assert_eq!(step, step2, "method {m} roundtrip");
            assert_eq!(back.method, m, "method index preserved");
        }
    }

    #[test]
    fn summaries_match_mock_format() {
        let seq = make_step(&default_form(0)).unwrap();
        assert_eq!(method_name(&seq), "連番");
        assert_eq!(step_summary(&seq), "末尾に 3桁（1〜）");
        let rep = make_step(&RenameForm {
            t1: "IMG".into(),
            t2: "画像".into(),
            ..default_form(7)
        })
        .unwrap();
        assert_eq!(step_summary(&rep), "「IMG」→「画像」");
    }
}
