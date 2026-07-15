//! リネーム方式（ステップ）の純粋変換（仕様 §3.10.1 のリネーム方式表）。
//!
//! 各ステップは拡張子を除いたファイル名本体（[`NamePair::stem`]）に適用する。例外は
//! 「大文字・小文字」で拡張子を対象に選んだ場合と、連番の付加位置。すべて副作用のない
//! 純関数で、入力を変更せず新しい [`NamePair`] を返す。

use std::collections::HashMap;

use percent_encoding::percent_decode_str;
use regex::Regex;
use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

use crate::exif::{ExifDateFormat, ExifInfo, format_datetime};

/// 連番の付加位置。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SeqPosition {
    /// ファイル名本体の先頭。
    Prefix,
    /// ファイル名本体の末尾。
    Suffix,
}

/// 大文字・小文字変換の対象。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CaseTarget {
    /// ファイル名本体。
    Stem,
    /// 拡張子。
    Ext,
}

/// 大文字・小文字の変換方向。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CaseConvert {
    /// 大文字へ。
    Upper,
    /// 小文字へ。
    Lower,
}

/// リネーム方式（仕様 §3.10.1 の 11 方式）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RenameStep {
    /// 連番（位置・桁数・開始番号・区切り）。本体の先頭または末尾に付加する。
    Sequence {
        position: SeqPosition,
        digits: u8,
        start: u64,
        sep: String,
    },
    /// 文字列＋連番。本体を「文字列＋区切り＋連番」へ置き換える。
    StringSequence {
        text: String,
        digits: u8,
        start: u64,
        sep: String,
    },
    /// 日時＋連番。本体を「日時＋区切り＋連番」へ置き換える（`date`＝事前計算済み `YYYYMMDD`）。
    DateSequence {
        date: String,
        digits: u8,
        start: u64,
        sep: String,
    },
    /// EXIF 情報による命名（トークン展開）。EXIF を持たないファイルは変更しない。
    ExifName {
        pattern: String,
        date_format: ExifDateFormat,
        digits: u8,
        start: u64,
    },
    /// 大文字・小文字変換。
    ChangeCase {
        target: CaseTarget,
        convert: CaseConvert,
    },
    /// 文字列を削除（一致箇所をすべて）。
    RemoveString { text: String },
    /// 位置指定で削除（`start`＝先頭から 1 始まりの文字位置・`count`＝削除文字数）。
    RemoveRange { start: usize, count: usize },
    /// テキスト置換（単純・一致箇所をすべて）。
    ReplaceText { from: String, to: String },
    /// 正規表現置換（`replace` は `$1` 等のキャプチャ参照可）。
    ReplaceRegex { pattern: String, replace: String },
    /// URL デコード（`%XX` 形式を復号）。
    UrlDecode,
    /// 濁点・半濁点の結合（NFC 正規化。Mac 由来ファイル名対策）。
    CombineDiacritics,
}

/// 適用中のファイル名（本体と拡張子）。拡張子は先頭ドットなし、無拡張子は空文字。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamePair {
    pub stem: String,
    pub ext: String,
}

/// レシピ内の正規表現を一度だけコンパイルして使い回すキャッシュ（パターン文字列→[`Regex`]）。
///
/// `build_plan` が対象ファイルごとに再コンパイルしないよう、適用前に [`build_regex_cache`] で
/// 構築する。不正パターンはキャッシュに入らず、適用時は無変更となる。
pub(crate) type RegexCache = HashMap<String, Regex>;

/// ステップ適用時のファイル固有コンテキスト。
pub(crate) struct StepCtx<'a> {
    /// 対象集合内の 0 始まり序数（連番に使用）。
    pub index: u64,
    /// EXIF（無ければ `None`＝EXIF 命名は無変更）。
    pub exif: Option<&'a ExifInfo>,
    /// 元のファイル名本体（`{元名}` トークン用。パイプライン途中でも常に元の値）。
    pub original_stem: &'a str,
    /// コンパイル済み正規表現（`ReplaceRegex` 用・対象集合で共有）。
    pub regex: &'a RegexCache,
}

impl RenameStep {
    /// 1 ステップを名前に適用した結果を返す（イミュータブル: 新しい [`NamePair`]）。
    #[must_use]
    pub(crate) fn apply(&self, name: &NamePair, ctx: &StepCtx) -> NamePair {
        match self {
            RenameStep::Sequence {
                position,
                digits,
                start,
                sep,
            } => {
                let num = seq_num(*start, ctx.index, *digits);
                let stem = match position {
                    SeqPosition::Prefix => join(&num, sep, &name.stem),
                    SeqPosition::Suffix => join(&name.stem, sep, &num),
                };
                NamePair {
                    stem,
                    ext: name.ext.clone(),
                }
            }
            RenameStep::StringSequence {
                text,
                digits,
                start,
                sep,
            } => {
                let num = seq_num(*start, ctx.index, *digits);
                NamePair {
                    stem: join(text, sep, &num),
                    ext: name.ext.clone(),
                }
            }
            RenameStep::DateSequence {
                date,
                digits,
                start,
                sep,
            } => {
                let num = seq_num(*start, ctx.index, *digits);
                NamePair {
                    stem: join(date, sep, &num),
                    ext: name.ext.clone(),
                }
            }
            RenameStep::ExifName {
                pattern,
                date_format,
                digits,
                start,
            } => match ctx.exif {
                // EXIF を持つファイルのみ命名する（持たないファイルは変更しない）。
                Some(exif) if exif.is_present() => NamePair {
                    stem: expand_exif(
                        pattern,
                        exif,
                        *date_format,
                        *start,
                        ctx.index,
                        *digits,
                        ctx.original_stem,
                    ),
                    ext: name.ext.clone(),
                },
                _ => name.clone(),
            },
            RenameStep::ChangeCase { target, convert } => {
                let conv = |s: &str| match convert {
                    CaseConvert::Upper => s.to_uppercase(),
                    CaseConvert::Lower => s.to_lowercase(),
                };
                match target {
                    CaseTarget::Stem => NamePair {
                        stem: conv(&name.stem),
                        ext: name.ext.clone(),
                    },
                    CaseTarget::Ext => NamePair {
                        stem: name.stem.clone(),
                        ext: conv(&name.ext),
                    },
                }
            }
            RenameStep::RemoveString { text } => {
                if text.is_empty() {
                    return name.clone();
                }
                NamePair {
                    stem: name.stem.replace(text.as_str(), ""),
                    ext: name.ext.clone(),
                }
            }
            RenameStep::RemoveRange { start, count } => NamePair {
                stem: remove_range(&name.stem, *start, *count),
                ext: name.ext.clone(),
            },
            RenameStep::ReplaceText { from, to } => {
                if from.is_empty() {
                    return name.clone();
                }
                NamePair {
                    stem: name.stem.replace(from.as_str(), to),
                    ext: name.ext.clone(),
                }
            }
            RenameStep::ReplaceRegex { pattern, replace } => match ctx.regex.get(pattern) {
                Some(re) => NamePair {
                    stem: re.replace_all(&name.stem, replace.as_str()).into_owned(),
                    ext: name.ext.clone(),
                },
                // 未コンパイル（不正な正規表現＝キャッシュに無い）は無変更。
                // GUI は validate で適用前に警告する。
                None => name.clone(),
            },
            RenameStep::UrlDecode => NamePair {
                stem: percent_decode_str(&name.stem)
                    .decode_utf8_lossy()
                    .into_owned(),
                ext: name.ext.clone(),
            },
            RenameStep::CombineDiacritics => NamePair {
                stem: name.stem.nfc().collect(),
                ext: name.ext.clone(),
            },
        }
    }

    /// 適用前の妥当性検証（正規表現のコンパイル可否）。GUI のプレビュー前チェック用。
    pub fn validate(&self) -> Result<(), String> {
        if let RenameStep::ReplaceRegex { pattern, .. } = self {
            Regex::new(pattern).map_err(|e| format!("正規表現が不正です: {e}"))?;
        }
        Ok(())
    }
}

/// レシピのステップ群から正規表現キャッシュを構築する。
///
/// 同一パターンは 1 度だけコンパイルし、不正パターンは省く（適用時に無変更）。対象ファイル
/// ごとの再コンパイルを避けるため、`build_plan` がループ前に 1 度だけ呼ぶ。
pub(crate) fn build_regex_cache<'a>(steps: impl Iterator<Item = &'a RenameStep>) -> RegexCache {
    let mut cache = RegexCache::new();
    for step in steps {
        if let RenameStep::ReplaceRegex { pattern, .. } = step {
            // 空パターンは全位置マッチ（置換文字列が各文字間に挿入される）ため除外＝無変更。
            // GUI のインライン編集では入力途中に空になるのが正常系のため、エラーにしない。
            if pattern.is_empty() {
                continue;
            }
            if !cache.contains_key(pattern)
                && let Ok(re) = Regex::new(pattern)
            {
                cache.insert(pattern.clone(), re);
            }
        }
    }
    cache
}

/// 連番文字列（開始番号＋序数を桁数ゼロ詰め）。桁あふれは詰めずにそのまま表示する。
fn seq_num(start: u64, index: u64, digits: u8) -> String {
    let n = start.saturating_add(index);
    format!("{n:0width$}", width = digits as usize)
}

/// 区切りを挟んで連結する（区切りが空なら直結）。
fn join(a: &str, sep: &str, b: &str) -> String {
    let mut s = String::with_capacity(a.len() + sep.len() + b.len());
    s.push_str(a);
    s.push_str(sep);
    s.push_str(b);
    s
}

/// 文字（Unicode スカラ）単位で範囲を削除する。`start` は 1 始まりの文字位置。
///
/// 範囲外（`start` が文字数を超える）や `count==0` は無変更。日本語等の多バイト文字を
/// バイト境界で割らないよう char 単位で扱う。
fn remove_range(s: &str, start: usize, count: usize) -> String {
    if start == 0 || count == 0 {
        return s.to_string();
    }
    let from = start - 1;
    // `Vec<char>` を確保せず char_indices で削除範囲のバイト境界を求める。
    let mut indices = s.char_indices().map(|(i, _)| i);
    let Some(byte_from) = indices.nth(from) else {
        return s.to_string(); // start が文字数を超える = 範囲外
    };
    // from の次から count 文字進めた位置（末尾を超えるなら文字列末尾）。
    let byte_to = indices.nth(count - 1).unwrap_or(s.len());
    let mut out = String::with_capacity(s.len() - (byte_to - byte_from));
    out.push_str(&s[..byte_from]);
    out.push_str(&s[byte_to..]);
    out
}

/// `{...}` トークンを1パスで展開する（中間 String の連鎖確保を避ける）。
///
/// `{` を見つけるたび既知トークンの先頭一致を試し、一致すれば展開値を、しなければ
/// `{` をそのまま書き出して次へ進む。展開後の文字列は再走査しない。
fn expand_tokens(pattern: &str, tokens: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(pattern.len());
    let mut rest = pattern;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open..];
        if let Some((tok, val)) = tokens.iter().find(|(t, _)| after.starts_with(*t)) {
            out.push_str(val);
            rest = &after[tok.len()..];
        } else {
            out.push('{');
            rest = &after['{'.len_utf8()..];
        }
    }
    out.push_str(rest);
    out
}

/// EXIF トークンを展開する。
///
/// 利用可能トークン: `{撮影日時}`・`{メーカー}`・`{機種}`・`{レンズ}`・`{連番}`・`{元名}`。
/// 個別フィールドが欠けるトークンは空文字へ展開し、展開後に区切りを整理する
/// （[`tidy_separators`]）。整理の結果が空なら元名を維持する（空名を作らない）。
fn expand_exif(
    pattern: &str,
    exif: &ExifInfo,
    date_format: ExifDateFormat,
    start: u64,
    index: u64,
    digits: u8,
    original_stem: &str,
) -> String {
    let datetime = format_datetime(exif.datetime, date_format);
    let num = seq_num(start, index, digits);
    // トークン→展開値。先頭一致で1パス置換し、`replace` 連鎖による中間 String の
    // 多重確保を避ける（一括リネームで対象が数千件規模になっても確保は1回）。
    let tokens: [(&str, &str); 6] = [
        ("{撮影日時}", &datetime),
        ("{メーカー}", exif.make.as_deref().unwrap_or("")),
        ("{機種}", exif.model.as_deref().unwrap_or("")),
        ("{レンズ}", exif.lens.as_deref().unwrap_or("")),
        ("{連番}", &num),
        ("{元名}", original_stem),
    ];
    let expanded = expand_tokens(pattern, &tokens);
    let tidy = tidy_separators(&expanded);
    if tidy.is_empty() {
        original_stem.to_string()
    } else {
        tidy
    }
}

/// 区切り文字（`_`・`-`・空白）を整理する。
///
/// 欠損トークンが空文字へ展開されると `A__B` や先頭末尾の `_` が残るため、連続する区切りを
/// 最初の 1 文字へ圧縮し、先頭末尾の区切りを除去する（仕様 §3.10.1 EXIF 命名の表示品質）。
fn tidy_separators(s: &str) -> String {
    let is_sep = |c: char| c == '_' || c == '-' || c.is_whitespace();
    let mut out = String::with_capacity(s.len());
    let mut prev_sep = false;
    for c in s.chars() {
        if is_sep(c) {
            if !prev_sep {
                out.push(c);
            }
            prev_sep = true;
        } else {
            out.push(c);
            prev_sep = false;
        }
    }
    out.trim_matches(is_sep).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn pair(stem: &str, ext: &str) -> NamePair {
        NamePair {
            stem: stem.to_string(),
            ext: ext.to_string(),
        }
    }

    fn apply(step: &RenameStep, name: &NamePair, index: u64) -> NamePair {
        let regex = build_regex_cache(std::iter::once(step));
        let ctx = StepCtx {
            index,
            exif: None,
            original_stem: "orig",
            regex: &regex,
        };
        step.apply(name, &ctx)
    }

    #[test]
    fn sequence_prefix_and_suffix_pad_with_start() {
        let p = pair("photo", "jpg");
        let pre = apply(
            &RenameStep::Sequence {
                position: SeqPosition::Prefix,
                digits: 3,
                start: 1,
                sep: "_".into(),
            },
            &p,
            0,
        );
        assert_eq!(pre.stem, "001_photo");
        let suf = apply(
            &RenameStep::Sequence {
                position: SeqPosition::Suffix,
                digits: 2,
                start: 10,
                sep: "-".into(),
            },
            &p,
            5,
        );
        assert_eq!(suf.stem, "photo-15");
        // 拡張子は不変。
        assert_eq!(suf.ext, "jpg");
    }

    #[test]
    fn string_and_date_sequence_replace_stem() {
        let p = pair("anything", "png");
        let s = apply(
            &RenameStep::StringSequence {
                text: "img".into(),
                digits: 4,
                start: 1,
                sep: "".into(),
            },
            &p,
            2,
        );
        assert_eq!(s.stem, "img0003");
        let d = apply(
            &RenameStep::DateSequence {
                date: "20240309".into(),
                digits: 2,
                start: 1,
                sep: "_".into(),
            },
            &p,
            0,
        );
        assert_eq!(d.stem, "20240309_01");
    }

    #[test]
    fn change_case_targets_stem_or_ext() {
        let p = pair("Photo", "JPG");
        let lower_stem = apply(
            &RenameStep::ChangeCase {
                target: CaseTarget::Stem,
                convert: CaseConvert::Lower,
            },
            &p,
            0,
        );
        assert_eq!(lower_stem.stem, "photo");
        assert_eq!(lower_stem.ext, "JPG");
        let lower_ext = apply(
            &RenameStep::ChangeCase {
                target: CaseTarget::Ext,
                convert: CaseConvert::Lower,
            },
            &p,
            0,
        );
        assert_eq!(lower_ext.ext, "jpg");
        assert_eq!(lower_ext.stem, "Photo");
    }

    #[test]
    fn remove_string_and_range() {
        let removed = apply(
            &RenameStep::RemoveString {
                text: "_copy".into(),
            },
            &pair("file_copy_copy", "txt"),
            0,
        );
        assert_eq!(removed.stem, "file");
        // 1 始まり・3 文字目から 2 文字削除。
        let ranged = apply(
            &RenameStep::RemoveRange { start: 3, count: 2 },
            &pair("abcdef", "txt"),
            0,
        );
        assert_eq!(ranged.stem, "abef");
    }

    #[test]
    fn remove_range_is_char_aware_and_clamps() {
        // マルチバイト（日本語）でもバイト境界で割らない。
        let r = apply(
            &RenameStep::RemoveRange { start: 1, count: 2 },
            &pair("あいうえ", ""),
            0,
        );
        assert_eq!(r.stem, "うえ");
        // 範囲外は無変更。
        let oob = apply(
            &RenameStep::RemoveRange {
                start: 99,
                count: 5,
            },
            &pair("abc", ""),
            0,
        );
        assert_eq!(oob.stem, "abc");
    }

    #[test]
    fn replace_text_and_regex() {
        let t = apply(
            &RenameStep::ReplaceText {
                from: " ".into(),
                to: "_".into(),
            },
            &pair("my file name", "txt"),
            0,
        );
        assert_eq!(t.stem, "my_file_name");
        let re = apply(
            &RenameStep::ReplaceRegex {
                pattern: r"(\d+)".into(),
                replace: "#$1".into(),
            },
            &pair("ver12", "txt"),
            0,
        );
        assert_eq!(re.stem, "ver#12");
    }

    #[test]
    fn invalid_regex_leaves_name_unchanged() {
        let step = RenameStep::ReplaceRegex {
            pattern: "(".into(),
            replace: "x".into(),
        };
        assert!(step.validate().is_err());
        let r = apply(&step, &pair("keep", "txt"), 0);
        assert_eq!(r.stem, "keep");
    }

    #[test]
    fn url_decode_and_nfc() {
        let d = apply(&RenameStep::UrlDecode, &pair("a%20b%2Bc", "txt"), 0);
        assert_eq!(d.stem, "a b+c");
        // 分離した濁点（U+304B U+3099）を結合（U+304C）。
        let nfc = apply(
            &RenameStep::CombineDiacritics,
            &pair("\u{304b}\u{3099}", ""),
            0,
        );
        assert_eq!(nfc.stem, "\u{304c}");
    }

    #[test]
    fn exif_name_expands_tokens_when_present() {
        let exif = ExifInfo {
            datetime: Some(
                NaiveDate::from_ymd_opt(2024, 3, 9)
                    .unwrap()
                    .and_hms_opt(14, 5, 6)
                    .unwrap(),
            ),
            make: Some("FUJIFILM".into()),
            model: Some("X100V".into()),
            lens: None,
            gps: None,
        };
        let regex = RegexCache::new();
        let ctx = StepCtx {
            index: 0,
            exif: Some(&exif),
            original_stem: "DSCF0001",
            regex: &regex,
        };
        let step = RenameStep::ExifName {
            pattern: "{機種}_{撮影日時}_{連番}".into(),
            date_format: ExifDateFormat::DateCompact,
            digits: 3,
            start: 1,
        };
        let r = step.apply(&pair("DSCF0001", "jpg"), &ctx);
        assert_eq!(r.stem, "X100V_20240309_001");
    }

    #[test]
    fn sequence_with_zero_digits_uses_shortest_form() {
        // digits=0 はゼロ詰めなし＝最短表現（GUI が 0 を防ぐ想定だが挙動を固定）。
        let r = apply(
            &RenameStep::Sequence {
                position: SeqPosition::Suffix,
                digits: 0,
                start: 5,
                sep: "_".into(),
            },
            &pair("x", "txt"),
            0,
        );
        assert_eq!(r.stem, "x_5");
    }

    #[test]
    fn exif_name_tidies_separators_for_missing_tokens() {
        // レンズ欠損 → "X100V__001" にならず "X100V_001"。先頭末尾の区切りも除去。
        let exif = ExifInfo {
            datetime: None,
            make: None,
            model: Some("X100V".into()),
            lens: None,
            gps: None,
        };
        let regex = RegexCache::new();
        let ctx = StepCtx {
            index: 0,
            exif: Some(&exif),
            original_stem: "orig",
            regex: &regex,
        };
        let step = RenameStep::ExifName {
            pattern: "{機種}_{レンズ}_{連番}".into(),
            date_format: ExifDateFormat::DateCompact,
            digits: 3,
            start: 1,
        };
        assert_eq!(step.apply(&pair("orig", "jpg"), &ctx).stem, "X100V_001");
        // 全トークン欠損（撮影日時のみのパターンで日時なし）→ 空にせず元名を維持。
        let only_date = RenameStep::ExifName {
            pattern: "{撮影日時}".into(),
            date_format: ExifDateFormat::DateCompact,
            digits: 3,
            start: 1,
        };
        assert_eq!(only_date.apply(&pair("orig", "jpg"), &ctx).stem, "orig");
    }

    #[test]
    fn exif_name_without_exif_is_noop() {
        let step = RenameStep::ExifName {
            pattern: "{機種}".into(),
            date_format: ExifDateFormat::DateCompact,
            digits: 1,
            start: 1,
        };
        // exif=None → 無変更。
        let r = apply(&step, &pair("keep", "jpg"), 0);
        assert_eq!(r.stem, "keep");
    }
}
