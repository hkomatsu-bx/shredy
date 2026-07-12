# imgsim テストデータの出所（PROVENANCE）

画像類似度（S8・仕様 §3.12）の品質計測ハーネス（roadmap §1.1）用の**実写真フィクスチャ**。
いずれも **Wikimedia Commons** の **CC0 1.0 Universal（パブリックドメイン献呈）** 画像の
250px サムネイル。CC0 のためライセンス制約なく同梱・再頒布・公開できる。取得日 2026-07-12。

| ファイル | 出所（Wikimedia Commons file page） | ライセンス |
|---|---|---|
| `cc0_ceiling.jpg` | [File:! Bright Lights Ceiling !.jpg](https://commons.wikimedia.org/wiki/File:!_Bright_Lights_Ceiling_!.jpg) | CC0 1.0 |
| `cc0_tower.jpg` | [File:! Russisch-Orthodoxe Kirche (Dresden) Glockenturm.jpg](https://commons.wikimedia.org/wiki/File:!_Russisch-Orthodoxe_Kirche_(Dresden)_Glockenturm.jpg) | CC0 1.0 |
| `cc0_sculpture.jpg` | [File:" Grand Travers " de Nicolas Lavarenne.jpg](https://commons.wikimedia.org/wiki/File:%22_Grand_Travers_%22_de_Nicolas_Lavarenne.jpg) | CC0 1.0 |

- **ライセンス確認**: Wikimedia Commons API（`prop=imageinfo&iiprop=extmetadata`）で各ファイルの
  `LicenseShortName = "CC0"`・`LicenseUrl = http://creativecommons.org/publicdomain/zero/1.0/deed.en`
  を確認済み（2026-07-12）。カテゴリ `Category:CC-Zero` 所属。
- **用途**: pHash の識別力・クラスタリング品質を**実写真様のハッシュ分布**で計測する
  （手続き生成の合成パターンは DCT エネルギーが少数係数に集中し退化した疎ハッシュになり、
  実写真とハッシュ分布が乖離して検証に不適だったため。経緯は roadmap §1.1 と `audit.md`）。
- **選定基準**: 相互に視覚的が明確に異なる被写体（室内照明 / 教会の塔 / 屋外彫刻）を選び、
  「別画像は連結しない」検証を clean にする。
