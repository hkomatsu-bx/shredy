# テストデータの出所（PROVENANCE）

## sample.heic（1001 バイト）

- **内容**: HEVC コードの合成 HEIF/HEIC テスト画像（major brand `heic` / compatible `mif1,heic,miaf`、`hvc1`/`hvcC` を含む。AV1 ではない）。
- **用途**: `shredy-core` の `heic` feature（純 Rust `heic` クレートによる HEIC/HEIF 復号, 仕様 §3.12）の**デコード経路テスト専用**のベクタ。
- **出所**: [`imazen/heic`](https://github.com/imazen/heic) v0.1.6 の `testdata/synthetic/synth_8bit_q50.heic`（同プロジェクトが生成した合成テスト画像）。
- **ライセンス上の注意**: `imazen/heic` は `AGPL-3.0-only OR LicenseRef-Imazen-Commercial` のデュアルライセンス。本ファイルは**個人利用・非配布**を前提に、テストベクタとしてのみ同梱する（`deny.toml` の `licenses.exceptions` の根拠と同じ前提）。
  - **このリポジトリを公開・頒布する場合は、明確に CC0／パブリックドメインの HEIC サンプルへ差し替えること。**
