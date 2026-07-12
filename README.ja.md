# Shredy

[English](README.md) | 日本語

Windows 専用のポータブルな重複・類似ファイル整理ツール。複数フォルダをまたいで重複または類似するファイルを検出し、安全に削除・移動・整理する。**Rust 全面実装 / GUI（Slint）と CLI の 2 フロントエンド + 共通 `shredy-core` エンジン。**

- **バージョン:** 1.0.3
- **対象:** Windows 11（build 26100 以降・x64）専用
- **ライセンス:** GPL-3.0-only
- **言語:** Rust（edition 2021 / rust 1.97+）

> ごみ箱・`\\?\` ロングパス・リパースポイント判定・`explorer.exe` 連携などで Windows API に依存するため、**他 OS ではビルド・実行できない**。

---

## 主な用途

- 同一内容ファイルの重複排除（バイト単位の完全比較）
- バックアップコピーや旧ファイルの削除
- 類似画像のグルーピングと整理（pHash）
- Office ファイルのメタデータ差異を無視した内容同一性の検出
- 空ファイル・一時ファイル・破損ファイルのクリーンアップ
- フォルダ間の差分確認、大量ファイルの一括リネーム

## 設計方針

- レジストリ非使用のポータブル動作（xcopy 配置）
- 誤削除防止を優先した安全設計（ごみ箱削除が既定・重複グループ内で最低 1 件は残す）
- Unicode（NFC 正規化）・ロングパス（`\\?\`・最大 4096 字）の完全対応
- Rust による高速動作、NAS（UNC）配慮の I/O 並列度自動調整

---

## アーキテクチャ

共通エンジン `shredy-core` に 2 フロントエンド（CLI / GUI）が単方向依存し、Windows 固有処理は最下層 `shredy-sys` に隔離する。

```
┌─────────────┐   ┌──────────────────────────────┐
│  shredy-cli │   │          shredy-gui           │
│ (shredy.exe)│   │ (shredy-gui.exe + lib)        │
│ 検索+一覧表示 │   │ UI(Slint)/bridge/wiring/state │
└──────┬──────┘   └───────────────┬──────────────┘
       │     どちらも core へ単方向依存   │
       └────────────┬────────────────┘
                    ▼
         ┌────────────────────┐
         │     shredy-core      │  走査・比較戦略・選択・削除・
         │  (中核ロジック・純Rust) │  設定・ユーティリティ・ログ
         └──────────┬─────────┘
                    ▼
         ┌────────────────────┐
         │     shredy-sys       │  reparse 判定 / in-use 判定 /
         │  (Windows 低レベルAPI) │  タイムゾーン取得
         └────────────────────┘
```

| クレート | 役割 | 生成物 |
|---|---|---|
| `shredy-sys` | Windows 低レベル API ラッパ（reparse / in-use / タイムゾーン） | lib |
| `shredy-core` | 検索エンジン・比較戦略・選択・削除計画/実行・設定・キャッシュ・ユーティリティ・リネーム・ログ | lib |
| `shredy-cli` | 引数解析 → 検索 → 結果整形出力 | `shredy.exe` |
| `shredy-gui` | UI 定義（Slint）・型変換・イベント結線・状態管理 | `shredy-gui.exe` + lib |

削除・移動・手動削除リスト生成・破損ファイルの隔離移動などの破壊的操作は **GUI が担い、CLI には持たせない**（責務分離・安全優先）。

---

## ビルド・テスト・実行

Cargo ワークスペース。**Windows 環境が必須**。

```bash
cargo build [--release]                                  # release は windows_subsystem=windows + サイズ最適 profile
cargo test --workspace                                   # -p <crate> / <name> で絞り込み
cargo clippy --workspace --all-targets -- -D warnings    # 警告をエラー扱い
cargo fmt --all                                          # --check で検証
cargo bench -p shredy-core                               # criterion ベンチ
cargo deny check                                         # ライセンス・advisory 点検
```

release ビルドはサイズ最適化（`opt-level="s"` / `lto=fat` / `codegen-units=1` / `strip`）。`panic="abort"` は不採用（不正 `.xls` 解析時の内部 panic を `catch_unwind` で捕捉する安全装置を維持するため）。

---

## CLI の使い方

`shredy.exe` は**重複ファイル検索と結果一覧表示に特化**する。

```
shredy.exe [オプション...] [フォルダパス...]
```

フォルダパスを省略した場合は設定 `TargetFolders` の選択済みフォルダを対象にする。

| オプション | 説明 |
|---|---|
| `--config <path>` | 設定ファイル（`shredy.config`）のフォルダまたはファイルパスを指定 |
| `-S[0-8]` | 比較方式を指定（下表）。省略時は設定の `CompareMethod` |
| `--io-parallelism <N>` | I/O 並列度（NAS 時の自動制限を上書き） |
| `--version` | バージョン文字列を表示 |

### 比較方式（`-S`）

| 値 | 方式 |
|---|---|
| 0 | BLAKE3 ハッシュ |
| 1 | SHA-256 ハッシュ |
| 2 | XXH3-128 ハッシュ |
| 3 | 部分一致（範囲 XXH3） |
| 4 | 名前 + サイズ + 日付 |
| 5 | 名前 + サイズ |
| 6 | 名前のみ |
| 7 | Office 類似度（正規化コンテンツのハッシュ） |
| 8 | 画像類似度（pHash → BK-tree → Union-Find） |

```bat
REM 指定フォルダを BLAKE3 で検索して結果を一覧表示
shredy.exe -S0 "D:\Photos" "D:\Photos_Backup"
```

---

## 安全設計（横断的制約）

| 制約 | 内容 |
|---|---|
| 安全優先 | 削除はごみ箱が既定。ディレクトリのリパースポイント（シンボリックリンク/ジャンクション）は実体巻き込みを避けてスキップ。ロック付き・使用中ファイルは対象外 |
| 「1つ残す」 | 重複グループ内で最低 1 件は残す（残す基準は `KeepFileRule` 6 通り） |
| ロングパス / Unicode | `\\?\`（最大 4096 字）/ NFC 正規化 + 大小無視比較 |
| キャンセル即応 | 走査・比較中も即時中断（再帰深度上限 64 でループ防止） |
| NAS 配慮 | 対象の過半が UNC なら I/O 並列度を自動 8 まで制限（`--io-parallelism` 明示時は尊重） |
| エラー継続 | アクセス不可ファイルは個別スキップし継続。致命的エラーのみ通知 |

---

## ドキュメント

実装事実の正典はソースコード（`crates/*`）。仕様と齟齬があればコードを優先する。

開発時に作成した設計・要件ドキュメント（AI-DLC 成果物＝機能仕様・受入基準・非機能要件・as-built アーキテクチャ記録）は役割を終えたため、`main` とは分離して `aidlc` ブランチにアーカイブしている。

---

## ライセンス

GPL-3.0-only。GUI の Slint（GPL-3.0 or 商用）と画像の HEIC デコード（AGPL-3.0 or 商用）を GPL/AGPL オプションで利用するため、本体も GPL-3.0 で配布する。詳細は `LICENSE` / `THIRD-PARTY-LICENSES.md` を参照。
