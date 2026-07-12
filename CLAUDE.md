# CLAUDE.md

Windows 専用ポータブル重複・類似ファイル整理ツール。GUI（Slint）と CLI の 2 フロントエンド＋共通 `shredy-core` エンジン。**Rust 全面実装・v1.0.3・GPL-3.0-only**。

機能・現状・アーキテクチャの詳細はソースコードに集約してある。本ファイルでは重複させず、**横断的に必要な事項のみ**記載する。

## 情報源（唯一の信頼できる記録）

- **実装事実の正典＝ソースコード**（`crates/*`）。仕様と齟齬があればコードを優先する
- `.claude/rules/rust/programming-guide.md` — Rust コーディング規約。**コードを書く前に参照**
- `.claude/rules/` — ECC ルール（コード規約・レビュー・テスト等）

> 開発時の AI-DLC 成果物（要件・設計・as-built 記録＝旧 `aidlc-docs/`）は役割を終えたため、`main` から分離し `aidlc` ブランチにアーカイブしてある。機能仕様・受入基準・非機能要件・アーキテクチャ記録や経緯を辿る場合は同ブランチを参照する。

## ビルド・テスト・実行

Cargo ワークスペース（edition 2021 / rust 1.97+）。4 クレート：`shredy-sys`（Win 低レベル API）→ `shredy-core`（中核エンジン）→ `shredy-cli`（`shredy.exe`）/ `shredy-gui`（`shredy-gui.exe`＋lib。ロジックは lib 側で単体テスト可）。

```bash
cargo build [--release]                                  # release は windows_subsystem=windows＋サイズ最適 profile（opt-level="s"/lto=fat/codegen-units=1/strip）
cargo test --workspace                                   # -p <crate> / <name> で絞り込み
cargo clippy --workspace --all-targets -- -D warnings    # 警告をエラー扱い
cargo fmt --all                                          # --check で検証
cargo bench -p shredy-core                               # criterion
cargo deny check                                         # ライセンス・advisory
```

対象は **Windows 11 build 26100 以降 (x64) 専用**（ごみ箱・`\\?\` ロングパス・リパースポイント判定・`explorer.exe` 連携で Win API に依存し、他 OS ではビルド・実行不可）。

## 実装メモ（参考）

主要クレート：

| 用途 | クレート |
|---|---|
| ハッシュ | `blake3` / `sha2` / `xxhash-rust`(xxh3) |
| 走査 / ごみ箱 | `walkdir` / `trash` |
| Office | `zip`+`quick-xml`（新形式）/ `cfb`+`calamine`（旧形式） |
| 画像・pHash | `image` / `heic`（HEIC/HEIF・任意フィーチャ）/ 自前 2D DCT（依存ツリー回避で直接実装） |
| 並列 / エラー | `rayon` / `thiserror`(core)・`anyhow`(bin) |
| GUI | `slint`（純 Rust・`fluent`・xcopy 適合） |

- **GUI=Slint**：`SearchEngine<'a>` は非 Send のため**ワーカースレッド内で構築**し、結果（Send な `Vec<DuplicateGroup>`）を `upgrade_in_event_loop` で UI へ返す。選択ロジックは core 委譲（GUI に再実装しない）。
- **HEIC/HEIF**：`shredy-core` の `heic` フィーチャ（既定 on・`--no-default-features` で除外可）。AGPL のため**個人利用・非配布前提**で利用（`deny.toml` 例外）。
- 設定キーの enum 値（`HashBlake3` 等 PascalCase）は JSON シリアライズ形式として維持。

## 横断的な設計制約（常に意識する。§ 番号は `aidlc` ブランチにアーカイブした旧 spec の節番号）

- **安全優先**：削除はごみ箱が既定。ディレクトリのリパースポイント（シンボリックリンク/ジャンクション）は実体巻き込みを避けてスキップ。ロック付きファイルは対象外。**`panic="abort"` は不採用**（`office/legacy.rs` の calamine 不正 .xls 内部 panic を `catch_unwind` で捕捉する保護が無効化されるため）。
- **「1つ残す」**（§3.4）：重複グループ内で最低 1 件は残す（残す基準は `KeepFileRule` 6 通り）。
- **GUI 選択の二概念**（§3.4）：行選択（ハイライト・単一）と削除選択（チェック・複数）は別概念。
- **ロングパス** `\\?\`（最大 4096 字）/ **Unicode NFC**（日本語・中韓・絵文字を正しく扱う）/ **キャンセル即応**（走査は再帰深度上限 64 でループ防止）/ **NAS 配慮**（対象の過半が UNC なら I/O 並列度を自動 8 まで制限・`--io-parallelism` 明示時は尊重）/ **フォルダ正規化**（登録フォルダが親子なら子を除外し二重列挙防止）。

## 規約（グローバル規約＋ `.claude/rules/` 準拠。特に重要な点）

- **応答は日本語**（コード・変数名・コミットメッセージの type は英語）。簡潔に、自明な説明は省く。
- **複雑なタスクは実装前に計画を提示し、承認後に着手**する。
- エラーは握りつぶさず意味あるメッセージ付きで処理。イミュータブル原則（既存を変更せず新規を返す）。
- Git：Conventional Commits（本文は日本語）。**確認なしの自動コミット・自動 push 禁止**。
- **README・ドキュメントを勝手に生成・変更しない**。テストコードを確認なしに削除・コメントアウトしない。動作する既存コードを理由なくリファクタリングしない。
