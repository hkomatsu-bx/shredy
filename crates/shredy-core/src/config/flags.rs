//! フラグ型設定（仕様 §5 `CorruptScanGroups` / `WrongExtensionGroups`）。
//!
//! カンマ区切りの PascalCase 文字列（例: `"ZipLike, Jpeg, Png, Heif"`）と
//! ビットフラグを相互変換する。未知トークンは寛容に無視する（後方互換）。

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

bitflags::bitflags! {
    /// 破損ファイル検出の対象グループ（仕様 §3.9.4）。
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct CorruptScanGroups: u8 {
        const ZIP_LIKE = 0b0000_0001;
        const JPEG     = 0b0000_0010;
        const PNG      = 0b0000_0100;
        const HEIF     = 0b0000_1000;
    }
}

bitflags::bitflags! {
    /// 拡張子不正ファイル検出の対象グループ（仕様 §3.9.5 / §5）。
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct WrongExtensionGroups: u8 {
        const IMAGE   = 0b0000_0001;
        const OFFICE  = 0b0000_0010;
        const VIDEO   = 0b0000_0100;
        const PDF     = 0b0000_1000;
        const AUDIO   = 0b0001_0000;
        const ARCHIVE = 0b0010_0000;
    }
}

/// 既定: 4 グループすべて（仕様 §5）。
impl Default for CorruptScanGroups {
    fn default() -> Self {
        CorruptScanGroups::all()
    }
}

/// 既定: `Image, Office, Video, Pdf`（`Audio`/`Archive` は含まない、仕様 §5）。
impl Default for WrongExtensionGroups {
    fn default() -> Self {
        WrongExtensionGroups::IMAGE
            | WrongExtensionGroups::OFFICE
            | WrongExtensionGroups::VIDEO
            | WrongExtensionGroups::PDF
    }
}

// 名前 ↔ フラグの対応表。
const CORRUPT_NAMES: &[(&str, CorruptScanGroups)] = &[
    ("ZipLike", CorruptScanGroups::ZIP_LIKE),
    ("Jpeg", CorruptScanGroups::JPEG),
    ("Png", CorruptScanGroups::PNG),
    ("Heif", CorruptScanGroups::HEIF),
];

const WRONG_EXT_NAMES: &[(&str, WrongExtensionGroups)] = &[
    ("Image", WrongExtensionGroups::IMAGE),
    ("Office", WrongExtensionGroups::OFFICE),
    ("Video", WrongExtensionGroups::VIDEO),
    ("Pdf", WrongExtensionGroups::PDF),
    ("Audio", WrongExtensionGroups::AUDIO),
    ("Archive", WrongExtensionGroups::ARCHIVE),
];

fn format_flags<F: Copy + PartialEq>(
    bits: F,
    table: &[(&str, F)],
    contains: impl Fn(F, F) -> bool,
) -> String {
    table
        .iter()
        .filter(|(_, flag)| contains(bits, *flag))
        .map(|(name, _)| *name)
        .collect::<Vec<_>>()
        .join(", ")
}

macro_rules! impl_flag_serde {
    ($ty:ty, $table:ident) => {
        impl fmt::Display for $ty {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                let s = format_flags(*self, $table, |bits, flag| bits.contains(flag));
                f.write_str(&s)
            }
        }

        impl FromStr for $ty {
            type Err = std::convert::Infallible;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                let mut acc = <$ty>::empty();
                for token in s.split(',') {
                    let token = token.trim();
                    if token.is_empty() {
                        continue;
                    }
                    if let Some((_, flag)) = $table
                        .iter()
                        .find(|(name, _)| name.eq_ignore_ascii_case(token))
                    {
                        acc |= *flag;
                    }
                    // 未知トークンは無視（後方互換）。
                }
                Ok(acc)
            }
        }

        impl Serialize for $ty {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(&self.to_string())
            }
        }

        impl<'de> Deserialize<'de> for $ty {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let s = String::deserialize(deserializer)?;
                // `FromStr::Err` は `Infallible` なので Err 腕は到達不能。
                // panic させず irrefutable let で取り出す（§7, Item 18）。
                let Ok(flags) = s.parse();
                Ok(flags)
            }
        }
    };
}

impl_flag_serde!(CorruptScanGroups, CORRUPT_NAMES);
impl_flag_serde!(WrongExtensionGroups, WRONG_EXT_NAMES);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corrupt_default_is_all_four() {
        assert_eq!(
            CorruptScanGroups::default().to_string(),
            "ZipLike, Jpeg, Png, Heif"
        );
    }

    #[test]
    fn wrong_ext_default_excludes_audio_archive() {
        assert_eq!(
            WrongExtensionGroups::default().to_string(),
            "Image, Office, Video, Pdf"
        );
    }

    #[test]
    fn parse_ignores_unknown_tokens() {
        let g: CorruptScanGroups = "Jpeg, Bogus, Png".parse().unwrap();
        assert_eq!(g, CorruptScanGroups::JPEG | CorruptScanGroups::PNG);
    }

    #[test]
    fn empty_string_is_empty_flags() {
        let g: WrongExtensionGroups = "".parse().unwrap();
        assert_eq!(g, WrongExtensionGroups::empty());
    }
}

#[cfg(test)]
mod prop_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        // PBT-02: フラグの文字列 round-trip（Display → FromStr）は恒等。
        #[test]
        fn corrupt_round_trip(bits in 0u8..16) {
            let g = CorruptScanGroups::from_bits_truncate(bits);
            let back: CorruptScanGroups = g.to_string().parse().unwrap();
            prop_assert_eq!(g, back);
        }

        #[test]
        fn wrong_ext_round_trip(bits in 0u8..64) {
            let g = WrongExtensionGroups::from_bits_truncate(bits);
            let back: WrongExtensionGroups = g.to_string().parse().unwrap();
            prop_assert_eq!(g, back);
        }
    }
}
