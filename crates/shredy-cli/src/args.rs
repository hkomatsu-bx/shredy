//! コマンドライン引数定義（仕様 §4）。

use std::path::PathBuf;

use clap::Parser;

/// Shredy CLI の引数。
#[derive(Debug, Parser)]
#[command(name = "shredy", version = shredy_core::VERSION, about = "重複・類似ファイル整理ツール (CLI)")]
pub struct CliArgs {
    /// 設定ファイル（shredy.config）のフォルダまたはパス。
    #[arg(long, value_name = "PATH")]
    pub config: Option<PathBuf>,

    /// 比較方式 (0=BLAKE3, 1=SHA-256, 2=XXH3, 3=部分一致, 4=名前+サイズ+日付,
    /// 5=名前+サイズ, 6=名前のみ, 7=Office 類似度, 8=画像類似度)。指定すると検索を実行する。
    #[arg(short = 'S', value_name = "METHOD")]
    pub method: Option<u8>,

    /// I/O 並列度（NAS 時の自動制限を上書き）。
    #[arg(long, value_name = "N")]
    pub io_parallelism: Option<usize>,

    /// 対象フォルダ（複数可）。
    #[arg(value_name = "FOLDER")]
    pub folders: Vec<PathBuf>,
}

impl CliArgs {
    /// 実引数を解析する。
    ///
    /// 引数が不正な場合・`--help` / `--version` 指定時は、clap がメッセージを出力して
    /// プロセスを終了する（戻らない）。
    #[must_use]
    pub fn parse_args() -> Self {
        <Self as Parser>::parse()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_method_and_folders() {
        let a = CliArgs::try_parse_from(["shredy", "-S0", "D:\\Photos", "D:\\Backup"]).unwrap();
        assert_eq!(a.method, Some(0));
        assert_eq!(a.folders.len(), 2);
    }

    #[test]
    fn parses_config_and_io_parallelism() {
        let a = CliArgs::try_parse_from(["shredy", "--io-parallelism", "4", "C:\\x"]).unwrap();
        assert_eq!(a.io_parallelism, Some(4));
        assert_eq!(a.folders.len(), 1);
    }

    #[test]
    fn rejects_removed_delete_flag() {
        // CLI は重複検索専用。削除系フラグは受理しない（未知の引数としてエラー）。
        assert!(CliArgs::try_parse_from(["shredy", "--delete", "C:\\x"]).is_err());
    }
}
