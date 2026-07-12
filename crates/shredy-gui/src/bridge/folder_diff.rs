//! フォルダ比較（§3.7）表示補助。

use shredy_core::{DiffEntry, DiffKind, FolderDiffSummary};

/// `filter` 値に従いエントリを絞り込む（0=全て / 1=OnlyA / 2=OnlyB / 3=Common / 4=SizeDiff）。
pub fn filter_entries(entries: &[DiffEntry], filter: i32) -> Vec<&DiffEntry> {
    entries
        .iter()
        .filter(|e| match filter {
            1 => e.kind == DiffKind::OnlyA,
            2 => e.kind == DiffKind::OnlyB,
            3 => e.kind == DiffKind::Common,
            4 => e.kind == DiffKind::SizeDiff,
            _ => true,
        })
        .collect()
}

/// 比較完了後のステータス文字列。
pub fn status_text(summary: &FolderDiffSummary) -> String {
    format!(
        "A のみ: {}件 / B のみ: {}件 / 共通: {}件 / サイズ差: {}件",
        summary.only_a, summary.only_b, summary.common, summary.size_diff
    )
}
