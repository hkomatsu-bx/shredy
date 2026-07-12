//! 検索結果の表示。

use shredy_core::DuplicateGroup;

/// 重複グループを標準出力へ表示する。
pub fn print_groups(groups: &[DuplicateGroup]) {
    if groups.is_empty() {
        println!("重複は見つかりませんでした。");
        return;
    }
    println!("{} 件の重複グループ:", groups.len());
    for (i, g) in groups.iter().enumerate() {
        println!("[{}] {} ({} 件)", i + 1, g.representative, g.files.len());
        for f in &g.files {
            println!("    {} ({} bytes)", f.path.as_path().display(), f.size.0);
        }
    }
}
