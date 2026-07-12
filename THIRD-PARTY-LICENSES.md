# サードパーティ ライセンス通知（THIRD-PARTY LICENSES）

本ファイルは Shredy のバイナリ／ソース配布物に同梱する、依存クレートのライセンス表記です。

- **Shredy 本体のライセンス**: GPL-3.0-only（全文は同梱の `LICENSE` を参照）。

- 依存クレートの全文ライセンスは各クレートの配布物（crates.io / 下記リポジトリ）に含まれます。

- 集計対象: 直接・推移を含む全依存 713 クレート（Shredy 自身のクレートを除く）。

- 生成元: `cargo metadata --all-features` の `license` フィールド（SPDX 式）。


---

## 1. 特記すべきコンポーネント

### Slint（GUI フレームワーク）

`slint` および `i-slint-*`（v1.16.1・13 クレート）は次のトリプルライセンスです:

> `GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0`

Shredy は **GPL-3.0-only オプション**を選択して利用します。Slint を GPL 以外で利用するには Slint 社の商用ライセンスが別途必要です。

### heic（HEIC/HEIF デコーダ）

`heic`（v0.1.6）は次のデュアルライセンスです:

> `AGPL-3.0-only OR LicenseRef-Imazen-Commercial`

Shredy は **AGPL-3.0-only オプション**を選択して利用します。AGPL-3.0 は GPLv3 第13条により GPL-3.0 と結合可能で、本体 GPL-3.0 と矛盾しません。AGPL のソース開示義務は、GPL-3.0 でソースを公開することで充足されます。`heic` フィーチャは `--no-default-features` でビルドから除外できます（その場合 HEIC/HEIF 非対応）。

> 注: AGPL の「ネットワーク経由利用者へのソース提供義務」(§13) は、ローカル動作の Shredy では実質的に発動しません。


---

## 2. ライセンス種別サマリ

| ライセンス（SPDX） | クレート数 |
|---|---:|
| `MIT OR Apache-2.0` | 307 |
| `MIT` | 162 |
| `Apache-2.0 OR MIT` | 59 |
| `MIT/Apache-2.0` | 29 |
| `Unicode-3.0` | 22 |
| `Apache-2.0` | 20 |
| `Zlib OR Apache-2.0 OR MIT` | 18 |
| `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | 18 |
| `GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0` | 13 |
| `BSD-3-Clause` | 9 |
| `MIT OR Apache-2.0 OR Zlib` | 9 |
| `BSD-2-Clause` | 6 |
| `Unlicense OR MIT` | 5 |
| `Apache-2.0/MIT` | 5 |
| `BSL-1.0` | 3 |
| `Zlib` | 3 |
| `MIT / Apache-2.0` | 2 |
| `CC0-1.0 OR MIT-0 OR Apache-2.0` | 2 |
| `BSD-3-Clause OR Apache-2.0` | 2 |
| `BSD-3-Clause OR MIT OR Apache-2.0` | 2 |
| `MIT OR Apache-2.0 OR LGPL-2.1-or-later` | 2 |
| `Unlicense/MIT` | 2 |
| `BSD-2-Clause OR Apache-2.0 OR MIT` | 2 |
| `0BSD OR MIT OR Apache-2.0` | 1 |
| `CC0-1.0 OR Apache-2.0 OR Apache-2.0 WITH LLVM-exception` | 1 |
| `Apache-2.0 AND MIT` | 1 |
| `(Apache-2.0 OR MIT) AND BSD-3-Clause` | 1 |
| `Apache-2.0 / MIT` | 1 |
| `AGPL-3.0-only OR LicenseRef-Imazen-Commercial` | 1 |
| `CC0-1.0 OR Apache-2.0` | 1 |
| `(MIT OR Apache-2.0) AND NCSA` | 1 |
| `ISC` | 1 |
| `MIT OR Zlib OR Apache-2.0` | 1 |
| `(MIT OR Apache-2.0) AND Unicode-3.0` | 1 |

> いずれも GPL-3.0 と互換（寛容ライセンス、または GPL/AGPL の互換条項による）。


---

## 3. 全依存クレート一覧

| クレート | バージョン | ライセンス | リポジトリ |
|---|---|---|---|
| ab_glyph | 0.2.32 | `Apache-2.0` | https://github.com/alexheretic/ab-glyph |
| ab_glyph_rasterizer | 0.1.10 | `Apache-2.0` | https://github.com/alexheretic/ab-glyph |
| accesskit | 0.22.0 | `MIT OR Apache-2.0` | https://github.com/AccessKit/accesskit |
| accesskit_atspi_common | 0.15.0 | `MIT OR Apache-2.0` | https://github.com/AccessKit/accesskit |
| accesskit_consumer | 0.32.0 | `MIT OR Apache-2.0` | https://github.com/AccessKit/accesskit |
| accesskit_macos | 0.23.0 | `MIT OR Apache-2.0` | https://github.com/AccessKit/accesskit |
| accesskit_unix | 0.18.0 | `MIT OR Apache-2.0` | https://github.com/AccessKit/accesskit |
| accesskit_windows | 0.30.0 | `MIT OR Apache-2.0` | https://github.com/AccessKit/accesskit |
| accesskit_winit | 0.30.0 | `Apache-2.0` | https://github.com/AccessKit/accesskit |
| adler2 | 2.0.1 | `0BSD OR MIT OR Apache-2.0` | https://github.com/oyvindln/adler2 |
| ahash | 0.8.12 | `MIT OR Apache-2.0` | https://github.com/tkaitchuck/ahash |
| aho-corasick | 1.1.4 | `Unlicense OR MIT` | https://github.com/BurntSushi/aho-corasick |
| aligned | 0.4.3 | `MIT OR Apache-2.0` | https://github.com/rust-embedded-community/aligned |
| aligned-vec | 0.6.4 | `MIT` | https://github.com/sarah-ek/aligned-vec/ |
| allocator-api2 | 0.2.21 | `MIT OR Apache-2.0` | https://github.com/zakarumych/allocator-api2 |
| android-activity | 0.6.1 | `MIT OR Apache-2.0` | https://github.com/rust-mobile/android-activity |
| android-properties | 0.2.2 | `MIT` | https://github.com/miklelappo/android-properties |
| android_system_properties | 0.1.5 | `MIT/Apache-2.0` | https://github.com/nical/android_system_properties |
| anes | 0.1.6 | `MIT OR Apache-2.0` | https://github.com/zrzka/anes-rs |
| annotate-snippets | 0.12.16 | `MIT OR Apache-2.0` | https://github.com/rust-lang/annotate-snippets-rs |
| anstream | 1.0.0 | `MIT OR Apache-2.0` | https://github.com/rust-cli/anstyle.git |
| anstyle | 1.0.14 | `MIT OR Apache-2.0` | https://github.com/rust-cli/anstyle.git |
| anstyle-parse | 1.0.0 | `MIT OR Apache-2.0` | https://github.com/rust-cli/anstyle.git |
| anstyle-query | 1.1.5 | `MIT OR Apache-2.0` | https://github.com/rust-cli/anstyle.git |
| anstyle-wincon | 3.0.11 | `MIT OR Apache-2.0` | https://github.com/rust-cli/anstyle.git |
| anyhow | 1.0.102 | `MIT OR Apache-2.0` | https://github.com/dtolnay/anyhow |
| arbitrary | 1.4.2 | `MIT OR Apache-2.0` | https://github.com/rust-fuzz/arbitrary/ |
| arboard | 3.6.1 | `MIT OR Apache-2.0` | https://github.com/1Password/arboard |
| archmage | 0.9.26 | `MIT OR Apache-2.0` | https://github.com/imazen/archmage |
| archmage-macros | 0.9.26 | `MIT OR Apache-2.0` | https://github.com/imazen/archmage |
| arg_enum_proc_macro | 0.3.4 | `MIT` | https://github.com/lu-zero/arg_enum_proc_macro |
| arrayref | 0.3.9 | `BSD-2-Clause` | https://github.com/droundy/arrayref |
| arrayvec | 0.7.6 | `MIT OR Apache-2.0` | https://github.com/bluss/arrayvec |
| as-raw-xcb-connection | 1.0.1 | `MIT OR Apache-2.0` | https://github.com/psychon/as-raw-xcb-connection |
| as-slice | 0.2.1 | `MIT OR Apache-2.0` | https://github.com/japaric/as-slice |
| ashpd | 0.11.1 | `MIT` | https://github.com/bilelmoussaoui/ashpd |
| async-broadcast | 0.7.2 | `MIT OR Apache-2.0` | https://github.com/smol-rs/async-broadcast |
| async-channel | 2.5.0 | `Apache-2.0 OR MIT` | https://github.com/smol-rs/async-channel |
| async-executor | 1.14.0 | `Apache-2.0 OR MIT` | https://github.com/smol-rs/async-executor |
| async-fs | 2.2.0 | `Apache-2.0 OR MIT` | https://github.com/smol-rs/async-fs |
| async-io | 2.6.0 | `Apache-2.0 OR MIT` | https://github.com/smol-rs/async-io |
| async-lock | 3.4.2 | `Apache-2.0 OR MIT` | https://github.com/smol-rs/async-lock |
| async-net | 2.0.0 | `Apache-2.0 OR MIT` | https://github.com/smol-rs/async-net |
| async-process | 2.5.0 | `Apache-2.0 OR MIT` | https://github.com/smol-rs/async-process |
| async-recursion | 1.1.1 | `MIT OR Apache-2.0` | https://github.com/dcchut/async-recursion |
| async-signal | 0.2.14 | `Apache-2.0 OR MIT` | https://github.com/smol-rs/async-signal |
| async-task | 4.7.1 | `Apache-2.0 OR MIT` | https://github.com/smol-rs/async-task |
| async-trait | 0.1.89 | `MIT OR Apache-2.0` | https://github.com/dtolnay/async-trait |
| atomic-waker | 1.1.2 | `Apache-2.0 OR MIT` | https://github.com/smol-rs/atomic-waker |
| atspi | 0.29.0 | `Apache-2.0 OR MIT` | https://github.com/odilia-app/atspi |
| atspi-common | 0.13.0 | `Apache-2.0 OR MIT` | https://github.com/odilia-app/atspi |
| atspi-proxies | 0.13.0 | `Apache-2.0 OR MIT` | https://github.com/odilia-app/atspi |
| auto_enums | 0.8.8 | `Apache-2.0 OR MIT` | https://github.com/taiki-e/auto_enums |
| autocfg | 1.5.1 | `Apache-2.0 OR MIT` | https://github.com/cuviper/autocfg |
| av-scenechange | 0.14.1 | `MIT` | https://github.com/rust-av/av-scenechange |
| av1-grain | 0.2.5 | `BSD-2-Clause` | https://github.com/rust-av/av1-grain |
| avif-serialize | 0.8.9 | `BSD-3-Clause` | https://github.com/kornelski/avif-serialize |
| base64 | 0.22.1 | `MIT OR Apache-2.0` | https://github.com/marshallpierce/rust-base64 |
| bincode | 2.0.1 | `MIT` | https://github.com/bincode-org/bincode |
| bindgen | 0.72.1 | `BSD-3-Clause` | https://github.com/rust-lang/rust-bindgen |
| bit-set | 0.8.0 | `Apache-2.0 OR MIT` | https://github.com/contain-rs/bit-set |
| bit-vec | 0.8.0 | `Apache-2.0 OR MIT` | https://github.com/contain-rs/bit-vec |
| bit_field | 0.10.3 | `Apache-2.0/MIT` | https://github.com/phil-opp/rust-bit-field |
| bitflags | 1.3.2 | `MIT/Apache-2.0` | https://github.com/bitflags/bitflags |
| bitflags | 2.13.0 | `MIT OR Apache-2.0` | https://github.com/bitflags/bitflags |
| bitstream-io | 4.10.0 | `MIT/Apache-2.0` | https://github.com/tuffy/bitstream-io |
| blake3 | 1.8.5 | `CC0-1.0 OR Apache-2.0 OR Apache-2.0 WITH LLVM-exception` | https://github.com/BLAKE3-team/BLAKE3 |
| block-buffer | 0.10.4 | `MIT OR Apache-2.0` | https://github.com/RustCrypto/utils |
| block2 | 0.5.1 | `MIT` | https://github.com/madsmtm/objc2 |
| block2 | 0.6.2 | `MIT` | https://github.com/madsmtm/objc2 |
| blocking | 1.6.2 | `Apache-2.0 OR MIT` | https://github.com/smol-rs/blocking |
| borsh | 1.6.1 | `MIT OR Apache-2.0` | https://github.com/near/borsh-rs |
| built | 0.8.1 | `MIT` | https://github.com/lukaslueg/built |
| bumpalo | 3.20.3 | `MIT OR Apache-2.0` | https://github.com/fitzgen/bumpalo |
| by_address | 1.2.1 | `MIT OR Apache-2.0` | https://github.com/mbrubeck/by_address |
| bytemuck | 1.25.0 | `Zlib OR Apache-2.0 OR MIT` | https://github.com/Lokathor/bytemuck |
| bytemuck_derive | 1.10.2 | `Zlib OR Apache-2.0 OR MIT` | https://github.com/Lokathor/bytemuck |
| byteorder | 1.5.0 | `Unlicense OR MIT` | https://github.com/BurntSushi/byteorder |
| byteorder-lite | 0.1.0 | `Unlicense OR MIT` | https://github.com/image-rs/byteorder-lite |
| bytes | 1.11.1 | `MIT` | https://github.com/tokio-rs/bytes |
| calamine | 0.26.1 | `MIT` | https://github.com/tafia/calamine |
| calloop | 0.13.0 | `MIT` | https://github.com/Smithay/calloop |
| calloop | 0.14.4 | `MIT` | https://github.com/Smithay/calloop |
| calloop-wayland-source | 0.3.0 | `MIT` | https://github.com/smithay/calloop-wayland-source |
| calloop-wayland-source | 0.4.1 | `MIT` | https://github.com/smithay/calloop-wayland-source |
| cast | 0.3.0 | `MIT OR Apache-2.0` | https://github.com/japaric/cast.rs |
| cc | 1.2.63 | `MIT OR Apache-2.0` | https://github.com/rust-lang/cc-rs |
| cexpr | 0.6.0 | `Apache-2.0/MIT` | https://github.com/jethrogb/rust-cexpr |
| cfb | 0.14.0 | `MIT` | https://github.com/mdsteele/rust-cfb |
| cfg-if | 1.0.4 | `MIT OR Apache-2.0` | https://github.com/rust-lang/cfg-if |
| cfg_aliases | 0.2.1 | `MIT` | https://github.com/katharostech/cfg_aliases |
| cgl | 0.3.2 | `MIT / Apache-2.0` | https://github.com/servo/cgl-rs |
| chrono | 0.4.45 | `MIT OR Apache-2.0` | https://github.com/chronotope/chrono |
| ciborium | 0.2.2 | `Apache-2.0` | https://github.com/enarx/ciborium |
| ciborium-io | 0.2.2 | `Apache-2.0` | https://github.com/enarx/ciborium |
| ciborium-ll | 0.2.2 | `Apache-2.0` | https://github.com/enarx/ciborium |
| clang-sys | 1.8.1 | `Apache-2.0` | https://github.com/KyleMayes/clang-sys |
| clap | 4.6.1 | `MIT OR Apache-2.0` | https://github.com/clap-rs/clap |
| clap_builder | 4.6.0 | `MIT OR Apache-2.0` | https://github.com/clap-rs/clap |
| clap_derive | 4.6.1 | `MIT OR Apache-2.0` | https://github.com/clap-rs/clap |
| clap_lex | 1.1.0 | `MIT OR Apache-2.0` | https://github.com/clap-rs/clap |
| clipboard-win | 5.4.1 | `BSL-1.0` | https://github.com/DoumanAsh/clipboard-win |
| clru | 0.6.3 | `MIT` | https://github.com/marmeladema/clru-rs |
| codepage | 0.1.2 | `Apache-2.0 OR MIT` | https://github.com/hsivonen/codepage |
| color_quant | 1.1.0 | `MIT` | https://github.com/image-rs/color_quant.git |
| colorchoice | 1.0.5 | `MIT OR Apache-2.0` | https://github.com/rust-cli/anstyle.git |
| combine | 4.6.7 | `MIT` | https://github.com/Marwes/combine |
| concurrent-queue | 2.5.0 | `Apache-2.0 OR MIT` | https://github.com/smol-rs/concurrent-queue |
| const-field-offset | 0.2.0 | `MIT OR Apache-2.0` | https://github.com/slint-ui/slint |
| const-field-offset-macro | 0.2.0 | `MIT OR Apache-2.0` | https://github.com/slint-ui/slint |
| constant_time_eq | 0.4.2 | `CC0-1.0 OR MIT-0 OR Apache-2.0` | https://github.com/cesarb/constant_time_eq |
| convert_case | 0.10.0 | `MIT` | https://github.com/rutrum/convert-case |
| copypasta | 0.10.2 | `MIT / Apache-2.0` | https://github.com/alacritty/copypasta |
| core-foundation | 0.10.1 | `MIT OR Apache-2.0` | https://github.com/servo/core-foundation-rs |
| core-foundation | 0.9.4 | `MIT OR Apache-2.0` | https://github.com/servo/core-foundation-rs |
| core-foundation-sys | 0.8.7 | `MIT OR Apache-2.0` | https://github.com/servo/core-foundation-rs |
| core-graphics | 0.23.2 | `MIT OR Apache-2.0` | https://github.com/servo/core-foundation-rs |
| core-graphics-types | 0.1.3 | `MIT OR Apache-2.0` | https://github.com/servo/core-foundation-rs |
| core_maths | 0.1.1 | `MIT` | https://github.com/robertbastian/core_maths |
| countme | 3.0.1 | `MIT OR Apache-2.0` | https://github.com/matklad/countme |
| cpufeatures | 0.2.17 | `MIT OR Apache-2.0` | https://github.com/RustCrypto/utils |
| cpufeatures | 0.3.0 | `MIT OR Apache-2.0` | https://github.com/RustCrypto/utils |
| crc32fast | 1.5.0 | `MIT OR Apache-2.0` | https://github.com/srijs/rust-crc32fast |
| criterion | 0.5.1 | `Apache-2.0 OR MIT` | https://github.com/bheisler/criterion.rs |
| criterion-plot | 0.5.0 | `MIT/Apache-2.0` | https://github.com/bheisler/criterion.rs |
| critical-section | 1.2.0 | `MIT OR Apache-2.0` | https://github.com/rust-embedded/critical-section |
| crossbeam-channel | 0.5.15 | `MIT OR Apache-2.0` | https://github.com/crossbeam-rs/crossbeam |
| crossbeam-deque | 0.8.6 | `MIT OR Apache-2.0` | https://github.com/crossbeam-rs/crossbeam |
| crossbeam-epoch | 0.9.18 | `MIT OR Apache-2.0` | https://github.com/crossbeam-rs/crossbeam |
| crossbeam-utils | 0.8.21 | `MIT OR Apache-2.0` | https://github.com/crossbeam-rs/crossbeam |
| crunchy | 0.2.4 | `MIT` | https://github.com/eira-fransham/crunchy |
| crypto-common | 0.1.7 | `MIT OR Apache-2.0` | https://github.com/RustCrypto/traits |
| ctor | 0.10.1 | `Apache-2.0 OR MIT` | https://github.com/mmastrac/rust-ctor |
| ctrlc | 3.5.2 | `MIT/Apache-2.0` | https://github.com/Detegr/rust-ctrlc.git |
| cursor-icon | 1.2.0 | `MIT OR Apache-2.0 OR Zlib` | https://github.com/rust-windowing/cursor-icon |
| data-url | 0.3.2 | `MIT OR Apache-2.0` | https://github.com/servo/rust-url |
| deranged | 0.5.8 | `MIT OR Apache-2.0` | https://github.com/jhpratt/deranged |
| derive_arbitrary | 1.4.2 | `MIT OR Apache-2.0` | https://github.com/rust-fuzz/arbitrary |
| derive_more | 2.1.1 | `MIT` | https://github.com/JelteF/derive_more |
| derive_more-impl | 2.1.1 | `MIT` | https://github.com/JelteF/derive_more |
| derive_utils | 0.15.1 | `Apache-2.0 OR MIT` | https://github.com/taiki-e/derive_utils |
| digest | 0.10.7 | `MIT OR Apache-2.0` | https://github.com/RustCrypto/traits |
| dispatch | 0.2.0 | `MIT` | http://github.com/SSheldon/rust-dispatch |
| dispatch2 | 0.3.1 | `Zlib OR Apache-2.0 OR MIT` | https://github.com/madsmtm/objc2 |
| displaydoc | 0.2.6 | `MIT OR Apache-2.0` | https://github.com/yaahc/displaydoc |
| dlib | 0.5.3 | `MIT` | https://github.com/elinorbgr/dlib |
| downcast-rs | 1.2.1 | `MIT/Apache-2.0` | https://github.com/marcianx/downcast-rs |
| dpi | 0.1.2 | `Apache-2.0 AND MIT` | https://github.com/rust-windowing/winit |
| drm | 0.14.1 | `MIT` | https://github.com/Smithay/drm-rs |
| drm-ffi | 0.9.1 | `MIT` | https://github.com/Smithay/drm-rs |
| drm-fourcc | 2.2.0 | `MIT` | https://github.com/danielzfranklin/drm-fourcc-rs |
| drm-sys | 0.8.1 | `MIT` | https://github.com/Smithay/drm-rs |
| dtor | 0.8.1 | `Apache-2.0 OR MIT` | https://github.com/mmastrac/rust-ctor |
| dunce | 1.0.5 | `CC0-1.0 OR MIT-0 OR Apache-2.0` | https://gitlab.com/kornelski/dunce |
| either | 1.16.0 | `MIT OR Apache-2.0` | https://github.com/rayon-rs/either |
| encoding_rs | 0.8.35 | `(Apache-2.0 OR MIT) AND BSD-3-Clause` | https://github.com/hsivonen/encoding_rs |
| endi | 1.1.1 | `MIT` | https://github.com/zeenix/endi |
| enough | 0.4.4 | `MIT OR Apache-2.0` | https://github.com/imazen/enough |
| enumflags2 | 0.7.12 | `MIT OR Apache-2.0` | https://github.com/meithecatte/enumflags2 |
| enumflags2_derive | 0.7.12 | `MIT OR Apache-2.0` | https://github.com/meithecatte/enumflags2 |
| equator | 0.4.2 | `MIT` | https://github.com/sarah-ek/equator/ |
| equator-macro | 0.4.2 | `MIT` | https://github.com/sarah-ek/equator/ |
| equivalent | 1.0.2 | `Apache-2.0 OR MIT` | https://github.com/indexmap-rs/equivalent |
| errno | 0.3.14 | `MIT OR Apache-2.0` | https://github.com/lambda-fairy/rust-errno |
| error-code | 3.3.2 | `BSL-1.0` | https://github.com/DoumanAsh/error-code |
| euclid | 0.22.14 | `MIT OR Apache-2.0` | https://github.com/servo/euclid |
| event-listener | 5.4.1 | `Apache-2.0 OR MIT` | https://github.com/smol-rs/event-listener |
| event-listener-strategy | 0.5.4 | `Apache-2.0 OR MIT` | https://github.com/smol-rs/event-listener-strategy |
| exr | 1.74.0 | `BSD-3-Clause` | https://github.com/johannesvollmer/exrs |
| fastrand | 2.4.1 | `Apache-2.0 OR MIT` | https://github.com/smol-rs/fastrand |
| fax | 0.2.7 | `MIT` | https://github.com/pdf-rs/fax |
| fdeflate | 0.3.7 | `MIT OR Apache-2.0` | https://github.com/image-rs/fdeflate |
| femtovg | 0.23.2 | `MIT OR Apache-2.0` | https://github.com/femtovg/femtovg |
| field-offset | 0.3.6 | `MIT OR Apache-2.0` | https://github.com/Diggsey/rust-field-offset |
| filetime | 0.2.29 | `MIT/Apache-2.0` | https://github.com/alexcrichton/filetime |
| find-msvc-tools | 0.1.9 | `MIT OR Apache-2.0` | https://github.com/rust-lang/cc-rs |
| flate2 | 1.1.9 | `MIT OR Apache-2.0` | https://github.com/rust-lang/flate2-rs |
| float-cmp | 0.9.0 | `MIT` | https://github.com/mikedilger/float-cmp |
| fnv | 1.0.7 | `Apache-2.0 / MIT` | https://github.com/servo/rust-fnv |
| foldhash | 0.1.5 | `Zlib` | https://github.com/orlp/foldhash |
| foldhash | 0.2.0 | `Zlib` | https://github.com/orlp/foldhash |
| font-types | 0.11.3 | `MIT OR Apache-2.0` | https://github.com/googlefonts/fontations |
| fontdb | 0.23.0 | `MIT` | https://github.com/RazrFalcon/fontdb |
| fontique | 0.8.0 | `Apache-2.0 OR MIT` | https://github.com/linebender/parley |
| foreign-types | 0.5.0 | `MIT/Apache-2.0` | https://github.com/sfackler/foreign-types |
| foreign-types-macros | 0.2.3 | `MIT/Apache-2.0` | https://github.com/sfackler/foreign-types |
| foreign-types-shared | 0.3.1 | `MIT/Apache-2.0` | https://github.com/sfackler/foreign-types |
| form_urlencoded | 1.2.2 | `MIT OR Apache-2.0` | https://github.com/servo/rust-url |
| futures | 0.3.32 | `MIT OR Apache-2.0` | https://github.com/rust-lang/futures-rs |
| futures-channel | 0.3.32 | `MIT OR Apache-2.0` | https://github.com/rust-lang/futures-rs |
| futures-core | 0.3.32 | `MIT OR Apache-2.0` | https://github.com/rust-lang/futures-rs |
| futures-executor | 0.3.32 | `MIT OR Apache-2.0` | https://github.com/rust-lang/futures-rs |
| futures-io | 0.3.32 | `MIT OR Apache-2.0` | https://github.com/rust-lang/futures-rs |
| futures-lite | 2.6.1 | `Apache-2.0 OR MIT` | https://github.com/smol-rs/futures-lite |
| futures-macro | 0.3.32 | `MIT OR Apache-2.0` | https://github.com/rust-lang/futures-rs |
| futures-sink | 0.3.32 | `MIT OR Apache-2.0` | https://github.com/rust-lang/futures-rs |
| futures-task | 0.3.32 | `MIT OR Apache-2.0` | https://github.com/rust-lang/futures-rs |
| futures-util | 0.3.32 | `MIT OR Apache-2.0` | https://github.com/rust-lang/futures-rs |
| gbm | 0.18.0 | `MIT` | https://github.com/Smithay/gbm.rs |
| gbm-sys | 0.4.0 | `MIT` | https://github.com/Drakulix/gbm.rs/tree/master/gbm-sys |
| generic-array | 0.14.7 | `MIT` | https://github.com/fizyk20/generic-array.git |
| gethostname | 1.1.0 | `Apache-2.0` | https://codeberg.org/swsnr/gethostname.rs.git |
| getopts | 0.2.24 | `MIT OR Apache-2.0` | https://github.com/rust-lang/getopts |
| getrandom | 0.3.4 | `MIT OR Apache-2.0` | https://github.com/rust-random/getrandom |
| getrandom | 0.4.2 | `MIT OR Apache-2.0` | https://github.com/rust-random/getrandom |
| gif | 0.14.2 | `MIT OR Apache-2.0` | https://github.com/image-rs/image-gif |
| gl_generator | 0.14.0 | `Apache-2.0` | https://github.com/brendanzab/gl-rs/ |
| glob | 0.3.3 | `MIT OR Apache-2.0` | https://github.com/rust-lang/glob |
| glow | 0.17.0 | `MIT OR Apache-2.0 OR Zlib` | https://github.com/grovesNL/glow |
| glutin | 0.32.3 | `Apache-2.0` | https://github.com/rust-windowing/glutin |
| glutin-winit | 0.5.0 | `MIT` | https://github.com/rust-windowing/glutin |
| glutin_egl_sys | 0.7.1 | `Apache-2.0` | https://github.com/rust-windowing/glutin |
| glutin_glx_sys | 0.6.1 | `Apache-2.0` | https://github.com/rust-windowing/glutin |
| glutin_wgl_sys | 0.6.1 | `Apache-2.0` | https://github.com/rust-windowing/glutin |
| grid | 1.0.1 | `MIT` | https://github.com/becheran/grid |
| half | 2.7.1 | `MIT OR Apache-2.0` | https://github.com/VoidStarKat/half-rs |
| harfrust | 0.5.2 | `MIT` | https://github.com/harfbuzz/harfrust |
| hashbrown | 0.14.5 | `MIT OR Apache-2.0` | https://github.com/rust-lang/hashbrown |
| hashbrown | 0.15.5 | `MIT OR Apache-2.0` | https://github.com/rust-lang/hashbrown |
| hashbrown | 0.16.1 | `MIT OR Apache-2.0` | https://github.com/rust-lang/hashbrown |
| hashbrown | 0.17.1 | `MIT OR Apache-2.0` | https://github.com/rust-lang/hashbrown |
| heck | 0.5.0 | `MIT OR Apache-2.0` | https://github.com/withoutboats/heck |
| heic | 0.1.6 | `AGPL-3.0-only OR LicenseRef-Imazen-Commercial` | https://github.com/imazen/heic |
| hermit-abi | 0.3.9 | `MIT OR Apache-2.0` | https://github.com/hermit-os/hermit-rs |
| hermit-abi | 0.5.2 | `MIT OR Apache-2.0` | https://github.com/hermit-os/hermit-rs |
| hex | 0.4.3 | `MIT OR Apache-2.0` | https://github.com/KokaKiwi/rust-hex |
| htmlparser | 0.2.1 | `MIT OR Apache-2.0` | https://github.com/jdrouet/htmlparser.git |
| i-slint-backend-linuxkms | 1.16.1 | `GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0` | https://github.com/slint-ui/slint |
| i-slint-backend-selector | 1.16.1 | `GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0` | https://github.com/slint-ui/slint |
| i-slint-backend-winit | 1.16.1 | `GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0` | https://github.com/slint-ui/slint |
| i-slint-common | 1.16.1 | `GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0` | https://github.com/slint-ui/slint |
| i-slint-compiler | 1.16.1 | `GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0` | https://github.com/slint-ui/slint |
| i-slint-core | 1.16.1 | `GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0` | https://github.com/slint-ui/slint |
| i-slint-core-macros | 1.16.1 | `GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0` | https://github.com/slint-ui/slint |
| i-slint-renderer-femtovg | 1.16.1 | `GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0` | https://github.com/slint-ui/slint |
| i-slint-renderer-skia | 1.16.1 | `GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0` | https://github.com/slint-ui/slint |
| i-slint-renderer-software | 1.16.1 | `GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0` | https://github.com/slint-ui/slint |
| iana-time-zone | 0.1.65 | `MIT OR Apache-2.0` | https://github.com/strawlab/iana-time-zone |
| iana-time-zone-haiku | 0.1.2 | `MIT OR Apache-2.0` | https://github.com/strawlab/iana-time-zone |
| icu_collections | 2.2.0 | `Unicode-3.0` | https://github.com/unicode-org/icu4x |
| icu_locale | 2.2.0 | `Unicode-3.0` | https://github.com/unicode-org/icu4x |
| icu_locale_core | 2.2.0 | `Unicode-3.0` | https://github.com/unicode-org/icu4x |
| icu_locale_data | 2.2.0 | `Unicode-3.0` | https://github.com/unicode-org/icu4x |
| icu_normalizer | 2.2.0 | `Unicode-3.0` | https://github.com/unicode-org/icu4x |
| icu_normalizer_data | 2.2.0 | `Unicode-3.0` | https://github.com/unicode-org/icu4x |
| icu_properties | 2.2.0 | `Unicode-3.0` | https://github.com/unicode-org/icu4x |
| icu_properties_data | 2.2.0 | `Unicode-3.0` | https://github.com/unicode-org/icu4x |
| icu_provider | 2.2.0 | `Unicode-3.0` | https://github.com/unicode-org/icu4x |
| icu_segmenter | 2.2.0 | `Unicode-3.0` | https://github.com/unicode-org/icu4x |
| icu_segmenter_data | 2.2.0 | `Unicode-3.0` | https://github.com/unicode-org/icu4x |
| id-arena | 2.3.0 | `MIT/Apache-2.0` | https://github.com/fitzgen/id-arena |
| idna | 1.1.0 | `MIT OR Apache-2.0` | https://github.com/servo/rust-url/ |
| idna_adapter | 1.2.2 | `Apache-2.0 OR MIT` | https://github.com/hsivonen/idna_adapter |
| image | 0.25.10 | `MIT OR Apache-2.0` | https://github.com/image-rs/image |
| image-webp | 0.2.4 | `MIT OR Apache-2.0` | https://github.com/image-rs/image-webp |
| imagesize | 0.14.0 | `MIT` | https://github.com/Roughsketch/imagesize |
| img-parts | 0.4.0 | `MIT OR Apache-2.0` | https://github.com/paolobarbolini/img-parts |
| imgref | 1.12.2 | `CC0-1.0 OR Apache-2.0` | https://github.com/kornelski/imgref |
| indexmap | 2.14.0 | `Apache-2.0 OR MIT` | https://github.com/indexmap-rs/indexmap |
| input | 0.9.1 | `MIT` | https://github.com/Drakulix/input.rs |
| input-sys | 1.19.0 | `MIT` | https://github.com/Drakulix/input.rs/tree/master/input-sys |
| integer-sqrt | 0.1.5 | `Apache-2.0/MIT` | https://github.com/derekdreery/integer-sqrt-rs |
| interpolate_name | 0.2.4 | `MIT` | https://github.com/lu-zero/interpolate_name |
| io-lifetimes | 1.0.11 | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | https://github.com/sunfishcode/io-lifetimes |
| is-terminal | 0.4.17 | `MIT` | https://github.com/sunfishcode/is-terminal |
| is_terminal_polyfill | 1.70.2 | `MIT OR Apache-2.0` | https://github.com/polyfill-rs/is_terminal_polyfill |
| itertools | 0.10.5 | `MIT/Apache-2.0` | https://github.com/rust-itertools/itertools |
| itertools | 0.14.0 | `MIT OR Apache-2.0` | https://github.com/rust-itertools/itertools |
| itoa | 1.0.18 | `MIT OR Apache-2.0` | https://github.com/dtolnay/itoa |
| jni | 0.22.4 | `MIT OR Apache-2.0` | https://github.com/jni-rs/jni-rs |
| jni-macros | 0.22.4 | `MIT OR Apache-2.0` | https://github.com/jni-rs/jni-rs |
| jni-sys | 0.3.1 | `MIT OR Apache-2.0` | https://github.com/jni-rs/jni-sys |
| jni-sys | 0.4.1 | `MIT OR Apache-2.0` | https://github.com/jni-rs/jni-sys |
| jni-sys-macros | 0.4.1 | `MIT OR Apache-2.0` | https://github.com/jni-rs/jni-sys |
| jobserver | 0.1.34 | `MIT OR Apache-2.0` | https://github.com/rust-lang/jobserver-rs |
| js-sys | 0.3.99 | `MIT OR Apache-2.0` | https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/js-sys |
| kamadak-exif | 0.6.1 | `BSD-2-Clause` | https://github.com/kamadak/exif-rs |
| keyboard-types | 0.7.0 | `MIT OR Apache-2.0` | https://github.com/pyfisch/keyboard-types |
| khronos_api | 3.1.0 | `Apache-2.0` | https://github.com/brendanzab/gl-rs/ |
| kurbo | 0.13.1 | `Apache-2.0 OR MIT` | https://github.com/linebender/kurbo |
| lazy_static | 1.5.0 | `MIT OR Apache-2.0` | https://github.com/rust-lang-nursery/lazy-static.rs |
| leb128fmt | 0.1.0 | `MIT OR Apache-2.0` | https://github.com/bluk/leb128fmt |
| lebe | 0.5.3 | `BSD-3-Clause` | https://github.com/johannesvollmer/lebe |
| libc | 0.2.186 | `MIT OR Apache-2.0` | https://github.com/rust-lang/libc |
| libfuzzer-sys | 0.4.13 | `(MIT OR Apache-2.0) AND NCSA` | https://github.com/rust-fuzz/libfuzzer |
| libloading | 0.8.9 | `ISC` | https://github.com/nagisa/rust_libloading/ |
| libm | 0.2.16 | `MIT` | https://github.com/rust-lang/compiler-builtins |
| libredox | 0.1.17 | `MIT` | https://gitlab.redox-os.org/redox-os/libredox.git |
| libudev-sys | 0.1.4 | `MIT` | https://github.com/dcuddeback/libudev-sys |
| linear-srgb | 0.6.12 | `MIT OR Apache-2.0` | https://github.com/imazen/linear-srgb |
| linebender_resource_handle | 0.1.1 | `Apache-2.0 OR MIT` | https://github.com/linebender/raw_resource_handle |
| linked-hash-map | 0.5.6 | `MIT/Apache-2.0` | https://github.com/contain-rs/linked-hash-map |
| linked_hash_set | 0.1.6 | `Apache-2.0` | https://github.com/alexheretic/linked-hash-set |
| linux-raw-sys | 0.12.1 | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | https://github.com/sunfishcode/linux-raw-sys |
| linux-raw-sys | 0.4.15 | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | https://github.com/sunfishcode/linux-raw-sys |
| linux-raw-sys | 0.9.4 | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | https://github.com/sunfishcode/linux-raw-sys |
| litemap | 0.8.2 | `Unicode-3.0` | https://github.com/unicode-org/icu4x |
| lock_api | 0.4.14 | `MIT OR Apache-2.0` | https://github.com/Amanieu/parking_lot |
| log | 0.4.32 | `MIT OR Apache-2.0` | https://github.com/rust-lang/log |
| loop9 | 0.1.5 | `MIT` | https://gitlab.com/kornelski/loop9.git |
| lyon_algorithms | 1.0.20 | `MIT OR Apache-2.0` | https://github.com/nical/lyon |
| lyon_extra | 1.1.0 | `MIT OR Apache-2.0` | https://github.com/nical/lyon |
| lyon_geom | 1.0.19 | `MIT OR Apache-2.0` | https://github.com/nical/lyon |
| lyon_path | 1.0.19 | `MIT OR Apache-2.0` | https://github.com/nical/lyon |
| magetypes | 0.9.26 | `MIT OR Apache-2.0` | https://github.com/imazen/archmage |
| maybe-rayon | 0.1.1 | `MIT` | https://github.com/shssoichiro/maybe-rayon |
| memchr | 2.8.1 | `Unlicense OR MIT` | https://github.com/BurntSushi/memchr |
| memmap2 | 0.9.10 | `MIT OR Apache-2.0` | https://github.com/RazrFalcon/memmap2-rs |
| memoffset | 0.9.1 | `MIT` | https://github.com/Gilnaa/memoffset |
| minimal-lexical | 0.2.1 | `MIT/Apache-2.0` | https://github.com/Alexhuszagh/minimal-lexical |
| miniz_oxide | 0.8.9 | `MIT OR Zlib OR Apache-2.0` | https://github.com/Frommi/miniz_oxide/tree/master/miniz_oxide |
| moxcms | 0.8.1 | `BSD-3-Clause OR Apache-2.0` | https://github.com/awxkee/moxcms.git |
| muda | 0.18.0 | `Apache-2.0 OR MIT` | https://github.com/tauri-apps/muda |
| mutate_once | 0.1.2 | `BSD-2-Clause` | https://github.com/kamadak/mutate_once-rs |
| natord | 1.0.9 | `MIT` | https://github.com/lifthrasiir/rust-natord |
| ndk | 0.9.0 | `MIT OR Apache-2.0` | https://github.com/rust-mobile/ndk |
| ndk-context | 0.1.1 | `MIT OR Apache-2.0` | https://github.com/rust-windowing/android-ndk-rs |
| ndk-sys | 0.6.0+11769913 | `MIT OR Apache-2.0` | https://github.com/rust-mobile/ndk |
| new_debug_unreachable | 1.0.6 | `MIT` | https://github.com/mbrubeck/rust-debug-unreachable |
| nix | 0.30.1 | `MIT` | https://github.com/nix-rust/nix |
| nix | 0.31.3 | `MIT` | https://github.com/nix-rust/nix |
| no_std_io2 | 0.9.4 | `Apache-2.0 OR MIT` | https://github.com/wcampbell0x2a/no-std-io2 |
| nom | 7.1.3 | `MIT` | https://github.com/Geal/nom |
| nom | 8.0.0 | `MIT` | https://github.com/rust-bakery/nom |
| noop_proc_macro | 0.3.0 | `MIT` | https://github.com/lu-zero/noop_proc_macro |
| nu-ansi-term | 0.50.3 | `MIT` | https://github.com/nushell/nu-ansi-term |
| num-bigint | 0.4.6 | `MIT OR Apache-2.0` | https://github.com/rust-num/num-bigint |
| num-conv | 0.2.2 | `MIT OR Apache-2.0` | https://github.com/jhpratt/num-conv |
| num-derive | 0.4.2 | `MIT OR Apache-2.0` | https://github.com/rust-num/num-derive |
| num-integer | 0.1.46 | `MIT OR Apache-2.0` | https://github.com/rust-num/num-integer |
| num-rational | 0.4.2 | `MIT OR Apache-2.0` | https://github.com/rust-num/num-rational |
| num-traits | 0.2.19 | `MIT OR Apache-2.0` | https://github.com/rust-num/num-traits |
| num_enum | 0.7.6 | `BSD-3-Clause OR MIT OR Apache-2.0` | https://github.com/illicitonion/num_enum |
| num_enum_derive | 0.7.6 | `BSD-3-Clause OR MIT OR Apache-2.0` | https://github.com/illicitonion/num_enum |
| objc-sys | 0.3.5 | `MIT` | https://github.com/madsmtm/objc2 |
| objc2 | 0.5.2 | `MIT` | https://github.com/madsmtm/objc2 |
| objc2 | 0.6.4 | `MIT` | https://github.com/madsmtm/objc2 |
| objc2-app-kit | 0.2.2 | `MIT` | https://github.com/madsmtm/objc2 |
| objc2-app-kit | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | https://github.com/madsmtm/objc2 |
| objc2-cloud-kit | 0.2.2 | `MIT` | https://github.com/madsmtm/objc2 |
| objc2-cloud-kit | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | https://github.com/madsmtm/objc2 |
| objc2-contacts | 0.2.2 | `MIT` | https://github.com/madsmtm/objc2 |
| objc2-core-data | 0.2.2 | `MIT` | https://github.com/madsmtm/objc2 |
| objc2-core-data | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | https://github.com/madsmtm/objc2 |
| objc2-core-foundation | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | https://github.com/madsmtm/objc2 |
| objc2-core-graphics | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | https://github.com/madsmtm/objc2 |
| objc2-core-image | 0.2.2 | `MIT` | https://github.com/madsmtm/objc2 |
| objc2-core-image | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | https://github.com/madsmtm/objc2 |
| objc2-core-location | 0.2.2 | `MIT` | https://github.com/madsmtm/objc2 |
| objc2-core-text | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | https://github.com/madsmtm/objc2 |
| objc2-core-video | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | https://github.com/madsmtm/objc2 |
| objc2-encode | 4.1.0 | `MIT` | https://github.com/madsmtm/objc2 |
| objc2-foundation | 0.2.2 | `MIT` | https://github.com/madsmtm/objc2 |
| objc2-foundation | 0.3.2 | `MIT` | https://github.com/madsmtm/objc2 |
| objc2-io-surface | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | https://github.com/madsmtm/objc2 |
| objc2-link-presentation | 0.2.2 | `MIT` | https://github.com/madsmtm/objc2 |
| objc2-metal | 0.2.2 | `MIT` | https://github.com/madsmtm/objc2 |
| objc2-metal | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | https://github.com/madsmtm/objc2 |
| objc2-quartz-core | 0.2.2 | `MIT` | https://github.com/madsmtm/objc2 |
| objc2-quartz-core | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | https://github.com/madsmtm/objc2 |
| objc2-symbols | 0.2.2 | `MIT` | https://github.com/madsmtm/objc2 |
| objc2-ui-kit | 0.2.2 | `MIT` | https://github.com/madsmtm/objc2 |
| objc2-ui-kit | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | https://github.com/madsmtm/objc2 |
| objc2-uniform-type-identifiers | 0.2.2 | `MIT` | https://github.com/madsmtm/objc2 |
| objc2-user-notifications | 0.2.2 | `MIT` | https://github.com/madsmtm/objc2 |
| once_cell | 1.21.4 | `MIT OR Apache-2.0` | https://github.com/matklad/once_cell |
| once_cell_polyfill | 1.70.2 | `MIT OR Apache-2.0` | https://github.com/polyfill-rs/once_cell_polyfill |
| oorandom | 11.1.5 | `MIT` | https://hg.sr.ht/~icefox/oorandom |
| orbclient | 0.3.55 | `MIT` | https://gitlab.redox-os.org/redox-os/orbclient |
| ordered-stream | 0.2.0 | `MIT OR Apache-2.0` | https://github.com/danieldg/ordered-stream |
| owned_ttf_parser | 0.25.1 | `Apache-2.0` | https://github.com/alexheretic/owned-ttf-parser |
| parking | 2.2.1 | `Apache-2.0 OR MIT` | https://github.com/smol-rs/parking |
| parking_lot | 0.12.5 | `MIT OR Apache-2.0` | https://github.com/Amanieu/parking_lot |
| parking_lot_core | 0.9.12 | `MIT OR Apache-2.0` | https://github.com/Amanieu/parking_lot |
| parlance | 0.1.0 | `Apache-2.0 OR MIT` | https://github.com/linebender/parley |
| parley | 0.8.0 | `Apache-2.0 OR MIT` | https://github.com/linebender/parley |
| parley_data | 0.8.0 | `Apache-2.0 OR MIT` | https://github.com/linebender/parley |
| paste | 1.0.15 | `MIT OR Apache-2.0` | https://github.com/dtolnay/paste |
| pastey | 0.1.1 | `MIT OR Apache-2.0` | https://github.com/as1100k/pastey |
| percent-encoding | 2.3.2 | `MIT OR Apache-2.0` | https://github.com/servo/rust-url/ |
| pico-args | 0.5.0 | `MIT` | https://github.com/RazrFalcon/pico-args |
| pin-project | 1.1.13 | `Apache-2.0 OR MIT` | https://github.com/taiki-e/pin-project |
| pin-project-internal | 1.1.13 | `Apache-2.0 OR MIT` | https://github.com/taiki-e/pin-project |
| pin-project-lite | 0.2.17 | `Apache-2.0 OR MIT` | https://github.com/taiki-e/pin-project-lite |
| pin-utils | 0.1.0 | `MIT OR Apache-2.0` | https://github.com/rust-lang-nursery/pin-utils |
| pin-weak | 1.1.0 | `MIT` | https://github.com/sixtyfpsui/pin-weak |
| piper | 0.2.5 | `MIT OR Apache-2.0` | https://github.com/smol-rs/piper |
| pkg-config | 0.3.33 | `MIT OR Apache-2.0` | https://github.com/rust-lang/pkg-config-rs |
| plain | 0.2.3 | `MIT/Apache-2.0` | https://github.com/randomites/plain |
| plotters | 0.3.7 | `MIT` | https://github.com/plotters-rs/plotters |
| plotters-backend | 0.3.7 | `MIT` | https://github.com/plotters-rs/plotters |
| plotters-svg | 0.3.7 | `MIT` | https://github.com/plotters-rs/plotters.git |
| png | 0.17.16 | `MIT OR Apache-2.0` | https://github.com/image-rs/image-png |
| png | 0.18.1 | `MIT OR Apache-2.0` | https://github.com/image-rs/image-png |
| polling | 3.11.0 | `Apache-2.0 OR MIT` | https://github.com/smol-rs/polling |
| pollster | 0.4.0 | `Apache-2.0/MIT` | https://github.com/zesterer/pollster |
| polycool | 0.4.0 | `MIT OR Apache-2.0` | https://github.com/linebender/kurbo |
| portable-atomic | 1.13.1 | `Apache-2.0 OR MIT` | https://github.com/taiki-e/portable-atomic |
| potential_utf | 0.1.5 | `Unicode-3.0` | https://github.com/unicode-org/icu4x |
| powerfmt | 0.2.0 | `MIT OR Apache-2.0` | https://github.com/jhpratt/powerfmt |
| ppv-lite86 | 0.2.21 | `MIT OR Apache-2.0` | https://github.com/cryptocorrosion/cryptocorrosion |
| prettyplease | 0.2.37 | `MIT OR Apache-2.0` | https://github.com/dtolnay/prettyplease |
| proc-macro-crate | 3.5.0 | `MIT OR Apache-2.0` | https://github.com/bkchr/proc-macro-crate |
| proc-macro2 | 1.0.106 | `MIT OR Apache-2.0` | https://github.com/dtolnay/proc-macro2 |
| profiling | 1.0.18 | `MIT OR Apache-2.0` | https://github.com/aclysma/profiling |
| profiling-procmacros | 1.0.18 | `MIT OR Apache-2.0` | https://github.com/aclysma/profiling |
| proptest | 1.11.0 | `MIT OR Apache-2.0` | https://github.com/proptest-rs/proptest |
| pulldown-cmark | 0.13.4 | `MIT` | https://github.com/raphlinus/pulldown-cmark |
| pulldown-cmark-escape | 0.11.0 | `MIT` | https://github.com/raphlinus/pulldown-cmark |
| pxfm | 0.1.29 | `BSD-3-Clause OR Apache-2.0` | https://github.com/awxkee/pxfm |
| qoi | 0.4.1 | `MIT/Apache-2.0` | https://github.com/aldanor/qoi-rust |
| quick-error | 1.2.3 | `MIT/Apache-2.0` | http://github.com/tailhook/quick-error |
| quick-error | 2.0.1 | `MIT/Apache-2.0` | http://github.com/tailhook/quick-error |
| quick-xml | 0.31.0 | `MIT` | https://github.com/tafia/quick-xml |
| quick-xml | 0.36.2 | `MIT` | https://github.com/tafia/quick-xml |
| quick-xml | 0.39.4 | `MIT` | https://github.com/tafia/quick-xml |
| quote | 1.0.45 | `MIT OR Apache-2.0` | https://github.com/dtolnay/quote |
| r-efi | 5.3.0 | `MIT OR Apache-2.0 OR LGPL-2.1-or-later` | https://github.com/r-efi/r-efi |
| r-efi | 6.0.0 | `MIT OR Apache-2.0 OR LGPL-2.1-or-later` | https://github.com/r-efi/r-efi |
| rand | 0.9.4 | `MIT OR Apache-2.0` | https://github.com/rust-random/rand |
| rand_chacha | 0.9.0 | `MIT OR Apache-2.0` | https://github.com/rust-random/rand |
| rand_core | 0.9.5 | `MIT OR Apache-2.0` | https://github.com/rust-random/rand |
| rand_xorshift | 0.4.0 | `MIT OR Apache-2.0` | https://github.com/rust-random/rngs |
| rav1e | 0.8.1 | `BSD-2-Clause` | https://github.com/xiph/rav1e/ |
| ravif | 0.13.0 | `BSD-3-Clause` | https://github.com/kornelski/cavif-rs |
| raw-window-handle | 0.6.2 | `MIT OR Apache-2.0 OR Zlib` | https://github.com/rust-windowing/raw-window-handle |
| raw-window-metal | 1.1.0 | `MIT OR Apache-2.0` | https://github.com/rust-windowing/raw-window-metal |
| rayon | 1.12.0 | `MIT OR Apache-2.0` | https://github.com/rayon-rs/rayon |
| rayon-core | 1.13.0 | `MIT OR Apache-2.0` | https://github.com/rayon-rs/rayon |
| read-fonts | 0.37.0 | `MIT OR Apache-2.0` | https://github.com/googlefonts/fontations |
| redox_syscall | 0.4.1 | `MIT` | https://gitlab.redox-os.org/redox-os/syscall |
| redox_syscall | 0.5.18 | `MIT` | https://gitlab.redox-os.org/redox-os/syscall |
| redox_syscall | 0.8.1 | `MIT` | https://gitlab.redox-os.org/redox-os/syscall |
| regex | 1.12.3 | `MIT OR Apache-2.0` | https://github.com/rust-lang/regex |
| regex-automata | 0.4.14 | `MIT OR Apache-2.0` | https://github.com/rust-lang/regex |
| regex-syntax | 0.8.10 | `MIT OR Apache-2.0` | https://github.com/rust-lang/regex |
| resvg | 0.47.0 | `Apache-2.0 OR MIT` | https://github.com/linebender/resvg |
| rfd | 0.15.4 | `MIT` | https://github.com/PolyMeilex/rfd |
| rgb | 0.8.53 | `MIT` | https://github.com/kornelski/rust-rgb |
| rowan | 0.16.1 | `MIT OR Apache-2.0` | https://github.com/rust-analyzer/rowan |
| roxmltree | 0.21.1 | `MIT OR Apache-2.0` | https://github.com/RazrFalcon/roxmltree |
| rspolib | 0.1.2 | `MIT` | https://github.com/mondeja/rspolib |
| rustc-hash | 1.1.0 | `Apache-2.0/MIT` | https://github.com/rust-lang-nursery/rustc-hash |
| rustc-hash | 2.1.2 | `Apache-2.0 OR MIT` | https://github.com/rust-lang/rustc-hash |
| rustc_version | 0.4.1 | `MIT OR Apache-2.0` | https://github.com/djc/rustc-version-rs |
| rustix | 0.38.44 | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | https://github.com/bytecodealliance/rustix |
| rustix | 1.1.4 | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | https://github.com/bytecodealliance/rustix |
| rustversion | 1.0.22 | `MIT OR Apache-2.0` | https://github.com/dtolnay/rustversion |
| rusty-fork | 0.3.1 | `MIT/Apache-2.0` | https://github.com/altsysrq/rusty-fork |
| rustybuzz | 0.20.1 | `MIT` | https://github.com/harfbuzz/rustybuzz |
| safe_arch | 1.0.0 | `Zlib OR Apache-2.0 OR MIT` | https://github.com/Lokathor/safe_arch |
| safe_unaligned_simd | 0.2.5 | `MIT OR Apache-2.0` | https://github.com/okaneco/safe_unaligned_simd |
| same-file | 1.0.6 | `Unlicense/MIT` | https://github.com/BurntSushi/same-file |
| scoped-tls | 1.0.1 | `MIT/Apache-2.0` | https://github.com/alexcrichton/scoped-tls |
| scoped-tls-hkt | 0.1.5 | `MIT/Apache-2.0` | https://github.com/Diggsey/scoped-tls-hkt |
| scopeguard | 1.2.0 | `MIT OR Apache-2.0` | https://github.com/bluss/scopeguard |
| sctk-adwaita | 0.10.1 | `MIT` | https://github.com/PolyMeilex/sctk-adwaita |
| semver | 1.0.28 | `MIT OR Apache-2.0` | https://github.com/dtolnay/semver |
| serde | 1.0.228 | `MIT OR Apache-2.0` | https://github.com/serde-rs/serde |
| serde_core | 1.0.228 | `MIT OR Apache-2.0` | https://github.com/serde-rs/serde |
| serde_derive | 1.0.228 | `MIT OR Apache-2.0` | https://github.com/serde-rs/serde |
| serde_json | 1.0.150 | `MIT OR Apache-2.0` | https://github.com/serde-rs/json |
| serde_repr | 0.1.20 | `MIT OR Apache-2.0` | https://github.com/dtolnay/serde-repr |
| serde_spanned | 1.1.1 | `MIT OR Apache-2.0` | https://github.com/toml-rs/toml |
| sha2 | 0.10.9 | `MIT OR Apache-2.0` | https://github.com/RustCrypto/hashes |
| sharded-slab | 0.1.7 | `MIT` | https://github.com/hawkw/sharded-slab |
| shlex | 1.3.0 | `MIT OR Apache-2.0` | https://github.com/comex/rust-shlex |
| shlex | 2.0.1 | `MIT OR Apache-2.0` | https://github.com/comex/rust-shlex |
| signal-hook-registry | 1.4.8 | `MIT OR Apache-2.0` | https://github.com/vorner/signal-hook |
| simd-adler32 | 0.3.9 | `MIT` | https://github.com/mcountryman/simd-adler32 |
| simd_cesu8 | 1.1.1 | `Apache-2.0 OR MIT` | https://github.com/seancroach/simd_cesu8 |
| simd_helpers | 0.1.0 | `MIT` | https://github.com/lu-zero/simd_helpers |
| simdutf8 | 0.1.5 | `MIT OR Apache-2.0` | https://github.com/rusticstuff/simdutf8 |
| simplecss | 0.2.2 | `Apache-2.0 OR MIT` | https://github.com/linebender/simplecss |
| siphasher | 1.0.3 | `MIT/Apache-2.0` | https://github.com/jedisct1/rust-siphash |
| skia-bindings | 0.90.0 | `MIT` | https://github.com/rust-skia/rust-skia |
| skia-safe | 0.90.0 | `MIT` | https://github.com/rust-skia/rust-skia |
| skrifa | 0.40.0 | `MIT OR Apache-2.0` | https://github.com/googlefonts/fontations |
| slab | 0.4.12 | `MIT` | https://github.com/tokio-rs/slab |
| slint | 1.16.1 | `GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0` | https://github.com/slint-ui/slint |
| slint-build | 1.16.1 | `GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0` | https://github.com/slint-ui/slint |
| slint-macros | 1.16.1 | `GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0` | https://github.com/slint-ui/slint |
| slotmap | 1.1.1 | `Zlib` | https://github.com/orlp/slotmap |
| smallvec | 1.15.1 | `MIT OR Apache-2.0` | https://github.com/servo/rust-smallvec |
| smithay-client-toolkit | 0.19.2 | `MIT` | https://github.com/smithay/client-toolkit |
| smithay-client-toolkit | 0.20.0 | `MIT` | https://github.com/smithay/client-toolkit |
| smithay-clipboard | 0.7.3 | `MIT` | https://github.com/smithay/smithay-clipboard |
| smol_str | 0.2.2 | `MIT OR Apache-2.0` | https://github.com/rust-analyzer/smol_str |
| smol_str | 0.3.6 | `MIT OR Apache-2.0` | https://github.com/rust-lang/rust-analyzer/tree/master/lib/smol_str |
| snafu | 0.8.9 | `MIT OR Apache-2.0` | https://github.com/shepmaster/snafu |
| snafu-derive | 0.8.9 | `MIT OR Apache-2.0` | https://github.com/shepmaster/snafu |
| softbuffer | 0.4.8 | `MIT OR Apache-2.0` | https://github.com/rust-windowing/softbuffer |
| spin_on | 0.1.1 | `Apache-2.0 OR MIT` | - |
| stable_deref_trait | 1.2.1 | `MIT OR Apache-2.0` | https://github.com/storyyeller/stable_deref_trait |
| static_assertions | 1.1.0 | `MIT OR Apache-2.0` | https://github.com/nvzqz/static-assertions-rs |
| strict-num | 0.1.1 | `MIT` | https://github.com/RazrFalcon/strict-num |
| strsim | 0.11.1 | `MIT` | https://github.com/rapidfuzz/strsim-rs |
| strum | 0.28.0 | `MIT` | https://github.com/Peternator7/strum |
| strum_macros | 0.28.0 | `MIT` | https://github.com/Peternator7/strum |
| svgtypes | 0.16.1 | `Apache-2.0 OR MIT` | https://github.com/linebender/svgtypes |
| swash | 0.2.7 | `Apache-2.0 OR MIT` | https://github.com/dfrg/swash |
| symlink | 0.1.0 | `MIT/Apache-2.0` | https://gitlab.com/chris-morgan/symlink |
| syn | 2.0.117 | `MIT OR Apache-2.0` | https://github.com/dtolnay/syn |
| synstructure | 0.13.2 | `MIT` | https://github.com/mystor/synstructure |
| sys-locale | 0.3.2 | `MIT OR Apache-2.0` | https://github.com/1Password/sys-locale |
| taffy | 0.9.2 | `MIT` | https://github.com/DioxusLabs/taffy |
| tar | 0.4.46 | `MIT OR Apache-2.0` | https://github.com/composefs/tar-rs |
| tempfile | 3.27.0 | `MIT OR Apache-2.0` | https://github.com/Stebalien/tempfile |
| text-size | 1.1.1 | `MIT OR Apache-2.0` | https://github.com/rust-analyzer/text-size |
| thiserror | 1.0.69 | `MIT OR Apache-2.0` | https://github.com/dtolnay/thiserror |
| thiserror | 2.0.18 | `MIT OR Apache-2.0` | https://github.com/dtolnay/thiserror |
| thiserror-impl | 1.0.69 | `MIT OR Apache-2.0` | https://github.com/dtolnay/thiserror |
| thiserror-impl | 2.0.18 | `MIT OR Apache-2.0` | https://github.com/dtolnay/thiserror |
| thread_local | 1.1.9 | `MIT OR Apache-2.0` | https://github.com/Amanieu/thread_local-rs |
| tiff | 0.11.3 | `MIT` | https://github.com/image-rs/image-tiff |
| time | 0.3.47 | `MIT OR Apache-2.0` | https://github.com/time-rs/time |
| time-core | 0.1.8 | `MIT OR Apache-2.0` | https://github.com/time-rs/time |
| time-macros | 0.2.27 | `MIT OR Apache-2.0` | https://github.com/time-rs/time |
| tiny-skia | 0.11.4 | `BSD-3-Clause` | https://github.com/RazrFalcon/tiny-skia |
| tiny-skia | 0.12.0 | `BSD-3-Clause` | https://github.com/linebender/tiny-skia |
| tiny-skia-path | 0.11.4 | `BSD-3-Clause` | https://github.com/RazrFalcon/tiny-skia/tree/master/path |
| tiny-skia-path | 0.12.0 | `BSD-3-Clause` | https://github.com/linebender/tiny-skia/tree/master/path |
| tiny-xlib | 0.2.5 | `MIT OR Apache-2.0 OR Zlib` | https://github.com/rust-windowing/tiny-xlib |
| tinystr | 0.8.3 | `Unicode-3.0` | https://github.com/unicode-org/icu4x |
| tinytemplate | 1.2.1 | `Apache-2.0 OR MIT` | https://github.com/bheisler/TinyTemplate |
| tinyvec | 1.11.0 | `Zlib OR Apache-2.0 OR MIT` | https://github.com/Lokathor/tinyvec |
| tinyvec_macros | 0.1.1 | `MIT OR Apache-2.0 OR Zlib` | https://github.com/Soveu/tinyvec_macros |
| toml | 0.9.12+spec-1.1.0 | `MIT OR Apache-2.0` | https://github.com/toml-rs/toml |
| toml | 1.1.2+spec-1.1.0 | `MIT OR Apache-2.0` | https://github.com/toml-rs/toml |
| toml_datetime | 0.7.5+spec-1.1.0 | `MIT OR Apache-2.0` | https://github.com/toml-rs/toml |
| toml_datetime | 1.1.1+spec-1.1.0 | `MIT OR Apache-2.0` | https://github.com/toml-rs/toml |
| toml_edit | 0.25.12+spec-1.1.0 | `MIT OR Apache-2.0` | https://github.com/toml-rs/toml |
| toml_parser | 1.1.2+spec-1.1.0 | `MIT OR Apache-2.0` | https://github.com/toml-rs/toml |
| toml_writer | 1.1.1+spec-1.1.0 | `MIT OR Apache-2.0` | https://github.com/toml-rs/toml |
| tracing | 0.1.44 | `MIT` | https://github.com/tokio-rs/tracing |
| tracing-appender | 0.2.5 | `MIT` | https://github.com/tokio-rs/tracing |
| tracing-attributes | 0.1.31 | `MIT` | https://github.com/tokio-rs/tracing |
| tracing-core | 0.1.36 | `MIT` | https://github.com/tokio-rs/tracing |
| tracing-log | 0.2.0 | `MIT` | https://github.com/tokio-rs/tracing |
| tracing-subscriber | 0.3.23 | `MIT` | https://github.com/tokio-rs/tracing |
| trash | 5.2.6 | `MIT` | https://github.com/ArturKovacs/trash |
| ttf-parser | 0.25.1 | `MIT OR Apache-2.0` | https://github.com/harfbuzz/ttf-parser |
| typed-index-collections | 3.5.0 | `MIT OR Apache-2.0` | https://github.com/zheland/typed-index-collections |
| typenum | 1.20.1 | `MIT OR Apache-2.0` | https://github.com/paholg/typenum |
| udev | 0.9.3 | `MIT` | https://github.com/Smithay/udev-rs |
| uds_windows | 1.2.1 | `MIT` | https://github.com/haraldh/rust_uds_windows |
| ultrahdr-core | 0.4.1 | `Apache-2.0` | https://github.com/imazen/ultrahdr |
| unarray | 0.1.4 | `MIT OR Apache-2.0` | https://github.com/cameron1024/unarray |
| unicase | 2.9.0 | `MIT OR Apache-2.0` | https://github.com/seanmonstar/unicase |
| unicode-bidi | 0.3.18 | `MIT OR Apache-2.0` | https://github.com/servo/unicode-bidi |
| unicode-bidi-mirroring | 0.4.0 | `MIT/Apache-2.0` | https://github.com/RazrFalcon/unicode-bidi-mirroring |
| unicode-ccc | 0.4.0 | `MIT/Apache-2.0` | https://github.com/RazrFalcon/unicode-ccc |
| unicode-ident | 1.0.24 | `(MIT OR Apache-2.0) AND Unicode-3.0` | https://github.com/dtolnay/unicode-ident |
| unicode-linebreak | 0.1.5 | `Apache-2.0` | https://github.com/axelf4/unicode-linebreak |
| unicode-normalization | 0.1.25 | `MIT OR Apache-2.0` | https://github.com/unicode-rs/unicode-normalization |
| unicode-properties | 0.1.4 | `MIT/Apache-2.0` | https://github.com/unicode-rs/unicode-properties |
| unicode-script | 0.5.8 | `MIT OR Apache-2.0` | https://github.com/unicode-rs/unicode-script |
| unicode-segmentation | 1.13.3 | `MIT OR Apache-2.0` | https://github.com/unicode-rs/unicode-segmentation |
| unicode-vo | 0.1.0 | `MIT/Apache-2.0` | https://github.com/RazrFalcon/unicode-vo |
| unicode-width | 0.2.2 | `MIT OR Apache-2.0` | https://github.com/unicode-rs/unicode-width |
| unicode-xid | 0.2.6 | `MIT OR Apache-2.0` | https://github.com/unicode-rs/unicode-xid |
| unty | 0.0.4 | `MIT OR Apache-2.0` | https://github.com/bincode-org/unty |
| url | 2.5.8 | `MIT OR Apache-2.0` | https://github.com/servo/rust-url |
| urlencoding | 2.1.3 | `MIT` | https://github.com/kornelski/rust_urlencoding |
| usvg | 0.47.0 | `Apache-2.0 OR MIT` | https://github.com/linebender/resvg |
| utf8_iter | 1.0.4 | `Apache-2.0 OR MIT` | https://github.com/hsivonen/utf8_iter |
| utf8parse | 0.2.2 | `Apache-2.0 OR MIT` | https://github.com/alacritty/vte |
| uuid | 1.23.2 | `Apache-2.0 OR MIT` | https://github.com/uuid-rs/uuid |
| v_frame | 0.3.9 | `BSD-2-Clause` | https://github.com/rust-av/v_frame |
| valuable | 0.1.1 | `MIT` | https://github.com/tokio-rs/valuable |
| version_check | 0.9.5 | `MIT/Apache-2.0` | https://github.com/SergioBenitez/version_check |
| vtable | 0.4.0 | `MIT OR Apache-2.0` | https://github.com/slint-ui/slint |
| vtable-macro | 0.4.0 | `MIT OR Apache-2.0` | https://github.com/slint-ui/slint |
| wait-timeout | 0.2.1 | `MIT/Apache-2.0` | https://github.com/alexcrichton/wait-timeout |
| walkdir | 2.5.0 | `Unlicense/MIT` | https://github.com/BurntSushi/walkdir |
| wasip2 | 1.0.3+wasi-0.2.9 | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | https://github.com/bytecodealliance/wasi-rs |
| wasip3 | 0.4.0+wasi-0.3.0-rc-2026-01-06 | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | https://github.com/bytecodealliance/wasi-rs |
| wasm-bindgen | 0.2.122 | `MIT OR Apache-2.0` | https://github.com/wasm-bindgen/wasm-bindgen |
| wasm-bindgen-futures | 0.4.72 | `MIT OR Apache-2.0` | https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/futures |
| wasm-bindgen-macro | 0.2.122 | `MIT OR Apache-2.0` | https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/macro |
| wasm-bindgen-macro-support | 0.2.122 | `MIT OR Apache-2.0` | https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/macro-support |
| wasm-bindgen-shared | 0.2.122 | `MIT OR Apache-2.0` | https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/shared |
| wasm-encoder | 0.244.0 | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | https://github.com/bytecodealliance/wasm-tools/tree/main/crates/wasm-encoder |
| wasm-metadata | 0.244.0 | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | https://github.com/bytecodealliance/wasm-tools/tree/main/crates/wasm-metadata |
| wasmparser | 0.244.0 | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | https://github.com/bytecodealliance/wasm-tools/tree/main/crates/wasmparser |
| wayland-backend | 0.3.15 | `MIT` | https://github.com/smithay/wayland-rs |
| wayland-client | 0.31.14 | `MIT` | https://github.com/smithay/wayland-rs |
| wayland-csd-frame | 0.3.0 | `MIT` | https://github.com/rust-windowing/wayland-csd-frame |
| wayland-cursor | 0.31.14 | `MIT` | https://github.com/smithay/wayland-rs |
| wayland-protocols | 0.32.12 | `MIT` | https://github.com/smithay/wayland-rs |
| wayland-protocols-experimental | 20250721.0.1 | `MIT` | https://github.com/smithay/wayland-rs |
| wayland-protocols-misc | 0.3.12 | `MIT` | https://github.com/smithay/wayland-rs |
| wayland-protocols-plasma | 0.3.12 | `MIT` | https://github.com/smithay/wayland-rs |
| wayland-protocols-wlr | 0.3.12 | `MIT` | https://github.com/smithay/wayland-rs |
| wayland-scanner | 0.31.10 | `MIT` | https://github.com/smithay/wayland-rs |
| wayland-sys | 0.31.11 | `MIT` | https://github.com/smithay/wayland-rs |
| web-sys | 0.3.99 | `MIT OR Apache-2.0` | https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/web-sys |
| web-time | 1.1.0 | `MIT OR Apache-2.0` | https://github.com/daxpedda/web-time |
| webbrowser | 1.2.1 | `MIT OR Apache-2.0` | https://github.com/amodm/webbrowser-rs |
| weezl | 0.1.12 | `MIT OR Apache-2.0` | https://github.com/image-rs/weezl |
| whereat | 0.1.5 | `MIT OR Apache-2.0` | https://github.com/lilith/whereat |
| wide | 1.5.0 | `Zlib OR Apache-2.0 OR MIT` | https://github.com/Lokathor/wide |
| winapi-util | 0.1.11 | `Unlicense OR MIT` | https://github.com/BurntSushi/winapi-util |
| winarm-cpufeatures | 0.1.2 | `MIT OR Apache-2.0` | https://github.com/imazen/winarm-cpufeatures |
| windows | 0.56.0 | `MIT OR Apache-2.0` | https://github.com/microsoft/windows-rs |
| windows | 0.61.3 | `MIT OR Apache-2.0` | https://github.com/microsoft/windows-rs |
| windows | 0.62.2 | `MIT OR Apache-2.0` | https://github.com/microsoft/windows-rs |
| windows-collections | 0.2.0 | `MIT OR Apache-2.0` | https://github.com/microsoft/windows-rs |
| windows-collections | 0.3.2 | `MIT OR Apache-2.0` | https://github.com/microsoft/windows-rs |
| windows-core | 0.56.0 | `MIT OR Apache-2.0` | https://github.com/microsoft/windows-rs |
| windows-core | 0.61.2 | `MIT OR Apache-2.0` | https://github.com/microsoft/windows-rs |
| windows-core | 0.62.2 | `MIT OR Apache-2.0` | https://github.com/microsoft/windows-rs |
| windows-future | 0.2.1 | `MIT OR Apache-2.0` | https://github.com/microsoft/windows-rs |
| windows-future | 0.3.2 | `MIT OR Apache-2.0` | https://github.com/microsoft/windows-rs |
| windows-implement | 0.56.0 | `MIT OR Apache-2.0` | https://github.com/microsoft/windows-rs |
| windows-implement | 0.60.2 | `MIT OR Apache-2.0` | https://github.com/microsoft/windows-rs |
| windows-interface | 0.56.0 | `MIT OR Apache-2.0` | https://github.com/microsoft/windows-rs |
| windows-interface | 0.59.3 | `MIT OR Apache-2.0` | https://github.com/microsoft/windows-rs |
| windows-link | 0.1.3 | `MIT OR Apache-2.0` | https://github.com/microsoft/windows-rs |
| windows-link | 0.2.1 | `MIT OR Apache-2.0` | https://github.com/microsoft/windows-rs |
| windows-numerics | 0.2.0 | `MIT OR Apache-2.0` | https://github.com/microsoft/windows-rs |
| windows-numerics | 0.3.1 | `MIT OR Apache-2.0` | https://github.com/microsoft/windows-rs |
| windows-result | 0.1.2 | `MIT OR Apache-2.0` | https://github.com/microsoft/windows-rs |
| windows-result | 0.3.4 | `MIT OR Apache-2.0` | https://github.com/microsoft/windows-rs |
| windows-result | 0.4.1 | `MIT OR Apache-2.0` | https://github.com/microsoft/windows-rs |
| windows-strings | 0.4.2 | `MIT OR Apache-2.0` | https://github.com/microsoft/windows-rs |
| windows-strings | 0.5.1 | `MIT OR Apache-2.0` | https://github.com/microsoft/windows-rs |
| windows-sys | 0.48.0 | `MIT OR Apache-2.0` | https://github.com/microsoft/windows-rs |
| windows-sys | 0.52.0 | `MIT OR Apache-2.0` | https://github.com/microsoft/windows-rs |
| windows-sys | 0.59.0 | `MIT OR Apache-2.0` | https://github.com/microsoft/windows-rs |
| windows-sys | 0.60.2 | `MIT OR Apache-2.0` | https://github.com/microsoft/windows-rs |
| windows-sys | 0.61.2 | `MIT OR Apache-2.0` | https://github.com/microsoft/windows-rs |
| windows-targets | 0.48.5 | `MIT OR Apache-2.0` | https://github.com/microsoft/windows-rs |
| windows-targets | 0.52.6 | `MIT OR Apache-2.0` | https://github.com/microsoft/windows-rs |
| windows-targets | 0.53.5 | `MIT OR Apache-2.0` | https://github.com/microsoft/windows-rs |
| windows-threading | 0.1.0 | `MIT OR Apache-2.0` | https://github.com/microsoft/windows-rs |
| windows-threading | 0.2.1 | `MIT OR Apache-2.0` | https://github.com/microsoft/windows-rs |
| windows_aarch64_gnullvm | 0.48.5 | `MIT OR Apache-2.0` | https://github.com/microsoft/windows-rs |
| windows_aarch64_gnullvm | 0.52.6 | `MIT OR Apache-2.0` | https://github.com/microsoft/windows-rs |
| windows_aarch64_gnullvm | 0.53.1 | `MIT OR Apache-2.0` | https://github.com/microsoft/windows-rs |
| windows_aarch64_msvc | 0.48.5 | `MIT OR Apache-2.0` | https://github.com/microsoft/windows-rs |
| windows_aarch64_msvc | 0.52.6 | `MIT OR Apache-2.0` | https://github.com/microsoft/windows-rs |
| windows_aarch64_msvc | 0.53.1 | `MIT OR Apache-2.0` | https://github.com/microsoft/windows-rs |
| windows_i686_gnu | 0.48.5 | `MIT OR Apache-2.0` | https://github.com/microsoft/windows-rs |
| windows_i686_gnu | 0.52.6 | `MIT OR Apache-2.0` | https://github.com/microsoft/windows-rs |
| windows_i686_gnu | 0.53.1 | `MIT OR Apache-2.0` | https://github.com/microsoft/windows-rs |
| windows_i686_gnullvm | 0.52.6 | `MIT OR Apache-2.0` | https://github.com/microsoft/windows-rs |
| windows_i686_gnullvm | 0.53.1 | `MIT OR Apache-2.0` | https://github.com/microsoft/windows-rs |
| windows_i686_msvc | 0.48.5 | `MIT OR Apache-2.0` | https://github.com/microsoft/windows-rs |
| windows_i686_msvc | 0.52.6 | `MIT OR Apache-2.0` | https://github.com/microsoft/windows-rs |
| windows_i686_msvc | 0.53.1 | `MIT OR Apache-2.0` | https://github.com/microsoft/windows-rs |
| windows_x86_64_gnu | 0.48.5 | `MIT OR Apache-2.0` | https://github.com/microsoft/windows-rs |
| windows_x86_64_gnu | 0.52.6 | `MIT OR Apache-2.0` | https://github.com/microsoft/windows-rs |
| windows_x86_64_gnu | 0.53.1 | `MIT OR Apache-2.0` | https://github.com/microsoft/windows-rs |
| windows_x86_64_gnullvm | 0.48.5 | `MIT OR Apache-2.0` | https://github.com/microsoft/windows-rs |
| windows_x86_64_gnullvm | 0.52.6 | `MIT OR Apache-2.0` | https://github.com/microsoft/windows-rs |
| windows_x86_64_gnullvm | 0.53.1 | `MIT OR Apache-2.0` | https://github.com/microsoft/windows-rs |
| windows_x86_64_msvc | 0.48.5 | `MIT OR Apache-2.0` | https://github.com/microsoft/windows-rs |
| windows_x86_64_msvc | 0.52.6 | `MIT OR Apache-2.0` | https://github.com/microsoft/windows-rs |
| windows_x86_64_msvc | 0.53.1 | `MIT OR Apache-2.0` | https://github.com/microsoft/windows-rs |
| winit | 0.30.13 | `Apache-2.0` | https://github.com/rust-windowing/winit |
| winnow | 0.7.15 | `MIT` | https://github.com/winnow-rs/winnow |
| winnow | 1.0.3 | `MIT` | https://github.com/winnow-rs/winnow |
| winresource | 0.1.31 | `MIT` | https://github.com/BenjaminRi/winresource |
| wit-bindgen | 0.51.0 | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | https://github.com/bytecodealliance/wit-bindgen |
| wit-bindgen | 0.57.1 | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | https://github.com/bytecodealliance/wit-bindgen |
| wit-bindgen-core | 0.51.0 | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | https://github.com/bytecodealliance/wit-bindgen |
| wit-bindgen-rust | 0.51.0 | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | https://github.com/bytecodealliance/wit-bindgen |
| wit-bindgen-rust-macro | 0.51.0 | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | https://github.com/bytecodealliance/wit-bindgen |
| wit-component | 0.244.0 | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | https://github.com/bytecodealliance/wasm-tools/tree/main/crates/wit-component |
| wit-parser | 0.244.0 | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | https://github.com/bytecodealliance/wasm-tools/tree/main/crates/wit-parser |
| write-fonts | 0.45.0 | `MIT OR Apache-2.0` | https://github.com/googlefonts/fontations |
| writeable | 0.6.3 | `Unicode-3.0` | https://github.com/unicode-org/icu4x |
| x11-clipboard | 0.9.3 | `MIT` | https://github.com/quininer/x11-clipboard |
| x11-dl | 2.21.0 | `MIT` | https://github.com/AltF02/x11-rs.git |
| x11rb | 0.13.2 | `MIT OR Apache-2.0` | https://github.com/psychon/x11rb |
| x11rb-protocol | 0.13.2 | `MIT OR Apache-2.0` | https://github.com/psychon/x11rb |
| xattr | 1.6.1 | `MIT OR Apache-2.0` | https://github.com/Stebalien/xattr |
| xcursor | 0.3.10 | `MIT` | https://github.com/esposm03/xcursor-rs |
| xkbcommon | 0.9.0 | `MIT` | https://github.com/rust-x-bindings/xkbcommon-rs |
| xkbcommon-dl | 0.4.2 | `MIT` | https://github.com/rust-windowing/xkbcommon-dl |
| xkeysym | 0.2.1 | `MIT OR Apache-2.0 OR Zlib` | https://github.com/notgull/xkeysym |
| xml-rs | 0.8.28 | `MIT` | https://github.com/kornelski/xml-rs |
| xmlwriter | 0.1.0 | `MIT` | https://github.com/RazrFalcon/xmlwriter |
| xxhash-rust | 0.8.15 | `BSL-1.0` | https://github.com/DoumanAsh/xxhash-rust |
| y4m | 0.8.0 | `MIT` | https://github.com/image-rs/y4m.git |
| yazi | 0.2.1 | `Apache-2.0 OR MIT` | https://github.com/dfrg/yazi |
| yeslogic-fontconfig-sys | 6.0.1 | `MIT` | https://github.com/yeslogic/fontconfig-rs |
| yoke | 0.8.3 | `Unicode-3.0` | https://github.com/unicode-org/icu4x |
| yoke-derive | 0.8.2 | `Unicode-3.0` | https://github.com/unicode-org/icu4x |
| zbus | 5.16.0 | `MIT` | https://github.com/z-galaxy/zbus/ |
| zbus-lockstep | 0.5.2 | `MIT` | https://github.com/luukvanderduim/zbus-lockstep |
| zbus-lockstep-macros | 0.5.2 | `MIT` | https://github.com/luukvanderduim/zbus-lockstep |
| zbus_macros | 5.16.0 | `MIT` | https://github.com/z-galaxy/zbus/ |
| zbus_names | 4.3.2 | `MIT` | https://github.com/z-galaxy/zbus/ |
| zbus_xml | 5.1.1 | `MIT` | https://github.com/z-galaxy/zbus/ |
| zeno | 0.3.3 | `Apache-2.0 OR MIT` | https://github.com/dfrg/zeno |
| zerocopy | 0.8.50 | `BSD-2-Clause OR Apache-2.0 OR MIT` | https://github.com/google/zerocopy |
| zerocopy-derive | 0.8.50 | `BSD-2-Clause OR Apache-2.0 OR MIT` | https://github.com/google/zerocopy |
| zerofrom | 0.1.8 | `Unicode-3.0` | https://github.com/unicode-org/icu4x |
| zerofrom-derive | 0.1.7 | `Unicode-3.0` | https://github.com/unicode-org/icu4x |
| zerotrie | 0.2.4 | `Unicode-3.0` | https://github.com/unicode-org/icu4x |
| zerovec | 0.11.6 | `Unicode-3.0` | https://github.com/unicode-org/icu4x |
| zerovec-derive | 0.11.3 | `Unicode-3.0` | https://github.com/unicode-org/icu4x |
| zip | 2.4.2 | `MIT` | https://github.com/zip-rs/zip2.git |
| zmij | 1.0.21 | `MIT` | https://github.com/dtolnay/zmij |
| zopfli | 0.8.3 | `Apache-2.0` | https://github.com/zopfli-rs/zopfli |
| zune-core | 0.5.1 | `MIT OR Apache-2.0 OR Zlib` | https://github.com/etemesi254/zune-image |
| zune-inflate | 0.2.54 | `MIT OR Apache-2.0 OR Zlib` | - |
| zune-jpeg | 0.5.15 | `MIT OR Apache-2.0 OR Zlib` | https://github.com/etemesi254/zune-image/tree/dev/crates/zune-jpeg |
| zvariant | 5.12.0 | `MIT` | https://github.com/z-galaxy/zbus/ |
| zvariant_derive | 5.12.0 | `MIT` | https://github.com/z-galaxy/zbus/ |
| zvariant_utils | 3.4.0 | `MIT` | https://github.com/z-galaxy/zbus/ |
