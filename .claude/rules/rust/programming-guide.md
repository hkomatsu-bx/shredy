# Rust プログラミングガイド

Rust のコーディング規約。本プロジェクトで Rust コードを書く際は本ガイドに従う。

## 出典と優先順位

複数の規約が衝突する場合、上にあるものを優先する。

1. **参考書『Effective Rust: 35 Specific Ways to Improve Your Rust Code』**（David Drysdale 著, O'Reilly, April 2024）。原著は著者により無料公開されており（<https://www.lurklurk.org/effective-rust/>）、本ガイドの項目番号・原題はこれに準拠する
2. **Rust 公式 Style Guide**（<https://doc.rust-lang.org/style-guide/>）— 整形・レイアウト
3. **Linux Kernel Rust Coding Guidelines**（<https://docs.kernel.org/rust/coding-guidelines.html>）— コメント／ドキュメント／`unsafe` 運用
4. プロジェクト共通規約（`.claude/rules/`）およびユーザーグローバル規約
5. **The Rust Programming Language（日本語版）§12.3「リファクタリングしてモジュール性とエラー処理を向上させる」**（<https://doc.rust-jp.rs/book-ja/ch12-03-improving-error-handling-and-modularity.html>）— バイナリ／ライブラリの**関心分離リファクタリング・プレイブック**（→ §14）。個々の設計判断（型・エラー・builder）は上位の Effective Rust 該当項目を根拠とし、本書はそれらを「`main` を薄く保ち、ロジックを lib へ寄せる」手順として束ねる位置づけ

### 補助リファレンス（規範ではなく参考）

設計の裏取りや実装の参考に用いる。規約の根拠としては 1〜5 を優先する。

- **Rust API Guidelines**（<https://rust-lang.github.io/api-guidelines/>）：公式の API 設計チェックリスト。命名・トレイト実装・エラー型・ドキュメントの設計判断を裏取りする（§2・§10・§11 を補強）
- **The Rustonomicon**（<https://doc.rust-lang.org/nomicon/>）：`unsafe` Rust の公式リファレンス。未定義動作と健全性の根拠（§8 を補強）
- **Rust Design Patterns**（<https://rust-unofficial.github.io/patterns/>）：イディオム・デザインパターン・アンチパターン集
- **Comprehensive Rust（Google）**（<https://google.github.io/comprehensive-rust/>）：無料の体系的コース。所有権・並行性・FFI の学習と裏取りに使う
- **Refactoring カタログ（Martin Fowler）**（<https://refactoring.com/catalog/>）：リファクタリング手法の定番カタログ（§16 の根拠）
- **The Rust Performance Book**（<https://nnethercote.github.io/perf-book/>）：Rust の性能最適化の体系的リファレンス（§16 の根拠）

> 設計判断（型・エラー・トレイト・所有権）は必ず Effective Rust の該当項目を根拠にする。整形・コメント様式は 2/3 に従う。補助リファレンスは規範ではなく参考にとどめる。

---

## 規約の読み方

各項目は「規約（命令形）＋ 根拠（`ER Item N` は Effective Rust の項目番号）」で記す。
`ER` は Effective Rust、`Style` は Rust Style Guide、`Kernel` は Linux Kernel Rust Coding Guidelines、`TRPL` は The Rust Programming Language を指す。`プロジェクト方針` は本プロジェクト固有の決定を指す（外部出典ではない）。
コード例の `// NG` は避けるべき書き方、`// OK` は推奨する書き方を示す。

---

## §1 型でデータ構造と不変条件を表現する

- 状態空間を `bool` やフラグの組で持たず `enum` で表現し、不正な状態を表現できないようにする（ER Item 1）。
- 「値が無いかもしれない」は番兵値ではなく `Option<T>` で表す（ER Item 1）。
- 異なる意味を持つ同じ基底型（ID、金額、秒数など）は newtype で包み、取り違えをコンパイルエラーにする（ER Item 6）。
- newtype は orphan rule の回避と単位の明示にも使う（ER Item 6）。

```rust
// NG: connected=false なのに port が Some になる不正状態を表現できてしまう
struct Conn { connected: bool, port: Option<u16> }
// OK: 不正状態を型で排除する
enum Conn { Disconnected, Connected { port: u16 } }

// NG: u64 同士はいくらでも取り違えられる
fn transfer(from: u64, to: u64, amount: u64) {}
// OK: newtype で取り違えを防ぐ
struct AccountId(u64);
struct Yen(u64);
fn transfer(from: AccountId, to: AccountId, amount: Yen) {}
```

---

## §2 トレイトで振る舞いを表現する

- 共通の振る舞いはトレイトで抽象化し、具体型への依存を避ける（ER Item 2）。
- 標準トレイトは可能な限り実装または `derive` する（ER Item 10）。
- `Debug` は原則すべての公開型に `derive` する。`Clone`／`PartialEq`／`Eq`／`Hash`／`Default` も意味が通る限り付与する（ER Item 10）。
- コピーが安価かつ意味的に妥当な型に限り `Copy` を付ける。所有権の移動に意味がある型には付けない（ER Item 10）。
- 解放処理（ファイル・ロック・接続）が必要な資源は `Drop` を実装し RAII で確定的に解放する（ER Item 11）。
- ジェネリクス（静的ディスパッチ、単相化）とトレイトオブジェクト `dyn Trait`（動的ディスパッチ）のトレードオフを理解して選ぶ（ER Item 12）。
- 異種の要素をコレクションに混在させる、またはコード量を抑えたい場合は `dyn Trait` を使う。性能とインライン化を優先する場合はジェネリクスを使う（ER Item 12）。
- トレイトはデフォルト実装を活用し、実装者が書くべき必須メソッドを最小化する（ER Item 13）。

```rust
// OK: 公開型には Debug を必ず derive する
#[derive(Debug, Clone, PartialEq)]
pub struct Point { x: i32, y: i32 }
```

---

## §3 エラー処理（Option / Result・panic 禁止）

- `Option`／`Result` は明示的な `match` で分解する前に変換メソッド（`map`・`and_then`・`unwrap_or`・`ok_or`・`?`）を優先する（ER Item 3）。
- エラー伝播は `?` 演算子で行い、`match` での手動伝播を書かない（ER Item 3）。
- ライブラリのエラー型は `enum` で表現し `std::error::Error` を実装する（`thiserror` を推奨）（ER Item 4）。
- アプリケーション最上位の文脈付きエラーには `anyhow`（または `Box<dyn Error>`）を使う（ER Item 4）。
- ライブラリコードでは `panic!`・`unwrap`・`expect` を使わず、`Result` を返す（ER Item 18）。
- `unwrap`／`expect` は、失敗が論理的にあり得ない箇所か、テスト・プロトタイプに限定する。使う場合は `expect` に理由を書く（ER Item 18）。
- パニックしうる関数は、その条件をドキュメントの `# Panics` セクションに記す（Kernel）。

```rust
// NG: match での手動伝播
let v = match parse(s) { Ok(v) => v, Err(e) => return Err(e) };
// OK: ? で伝播
let v = parse(s)?;

// OK: ライブラリのエラー型（thiserror）
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("ファイルが見つからない: {0}")]
    NotFound(String),
    #[error("入出力エラー")]
    Io(#[from] std::io::Error),
}
```

---

## §4 型変換

- 型変換は `From`／`Into` を実装し、呼び出し側は `Into` 境界で受ける（ER Item 5）。
- 失敗しうる変換は `TryFrom`／`TryInto` を実装し、`Result` を返す（ER Item 5）。
- `From` を実装すれば `Into` は自動で得られるため、`From` 側を実装する（ER Item 5）。
- 数値の `as` キャストは切り捨て・桁あふれを起こすため避け、`TryFrom`（`u32::try_from(x)` など）を使う（ER Item 5）。
- 参照の受け渡しは `AsRef`／`Borrow` で汎用化する（ER Item 5）。

```rust
// NG: as は桁あふれを黙って起こす
let small = big as u8;
// OK: 失敗を検出できる
let small = u8::try_from(big)?;
```

---

## §5 builder パターンで複雑な型を構築する

- フィールドが多い、または任意項目が多い型は builder パターンで構築する（ER Item 7）。
- 必須項目は builder の生成時に、任意項目はセッターで受け、`build()` で検証して完成させる（ER Item 7）。
- 引数の多い `new(...)` や、`Option` を並べた巨大なコンストラクタを避ける（ER Item 7）。

```rust
let req = RequestBuilder::new(url)   // 必須
    .timeout(Duration::from_secs(5)) // 任意
    .retries(3)                      // 任意
    .build()?;
```

---

## §6 所有権・参照・ライフタイム

- 参照・ポインタ型（`&T`・`&mut T`・`Box<T>`・`Rc<T>`・`Arc<T>`・`Cell`・`RefCell`）の使い分けを理解して選ぶ（ER Item 8）。
- 単一所有は `Box`、共有所有はシングルスレッドで `Rc`、マルチスレッドで `Arc` を使う（ER Item 8）。
- 内部可変性は `Cell`／`RefCell`（単一スレッド）、`Mutex`／`RwLock`（マルチスレッド）で表現する（ER Item 8）。
- ライフタイムは「参照が有効である期間の制約」であると理解し、注釈は借用関係を表すために最小限で書く（ER Item 14）。
- ボローチェッカーと戦わず、エラーが出たらまずデータ構造と所有権設計を見直す（ER Item 15）。
- 設計を変えずに解消したい場合の脱出口として `clone`・`Rc`／`Arc`・インデックス参照を使う。安易な多用は避ける（ER Item 15）。

---

## §7 イテレータ変換を使う

- 明示的な `for`／`while` ループより、イテレータ変換（`map`・`filter`・`fold`・`collect`・`sum`）を優先する（ER Item 9）。
- 添字アクセスのループは境界外アクセスの温床になるため、イテレータで置き換える（ER Item 9）。
- 副作用のみが目的の繰り返しは `for` を使う。変換・集約はイテレータを使う（ER Item 9）。

```rust
// NG: 添字ループ
let mut out = Vec::new();
for i in 0..xs.len() { if xs[i] > 0 { out.push(xs[i] * 2); } }
// OK: イテレータ変換
let out: Vec<_> = xs.iter().filter(|&&x| x > 0).map(|&x| x * 2).collect();
```

---

## §8 unsafe と並行性

- `unsafe` は原則として書かない。安全な抽象で代替できないか先に検討する（ER Item 16）。
- やむを得ず使う場合は、最小の範囲に閉じ、安全な API で包む（ER Item 16）。
- すべての `unsafe` ブロックの直前に `// SAFETY:` コメントを置き、なぜ未定義動作を起こさないかを説明する（Kernel）。
- `unsafe fn` は、呼び出し側が守るべき前提条件をドキュメントの `# Safety` セクションに記す（Kernel）。
- 共有状態の並行処理は注意して扱い、可能ならメッセージパッシング（チャネル）を優先する（ER Item 17）。
- 共有が必要な場合は `Arc<Mutex<T>>`／`Arc<RwLock<T>>` を使い、ロック範囲を最小化してデッドロックを避ける（ER Item 17）。
- スレッド間で渡す型は `Send`／`Sync` の意味を理解して扱う（ER Item 17）。

```rust
/// バッファ先頭の値を読み出す。
///
/// # Safety
///
/// 呼び出し側は `ptr` が有効でアラインされた `u32` を指すことを保証しなければならない。
pub unsafe fn read_head(ptr: *const u32) -> u32 {
    // SAFETY: ptr の有効性は呼び出し側が保証する。
    unsafe { *ptr }
}
```

---

## §9 リフレクション回避・過剰最適化回避

- `Any` によるダウンキャスト（実行時リフレクション）を避け、型・トレイトで設計する（ER Item 19）。
- 計測前の最適化を避ける。まず正しく書き、ボトルネックを計測してから最適化する（ER Item 20）。
- `clone` を過度に恐れない。借用設計が複雑になるくらいなら、まず `clone` で素直に書く（ER Item 20）。

---

## §10 依存関係と API 設計

- セマンティックバージョニングが約束する範囲（破壊的変更はメジャー更新でのみ起こる）を理解して依存を指定する（ER Item 21）。
- 公開する API の可視性は最小化し、`pub` は必要なものだけに付ける（ER Item 22）。
- ワイルドカードインポート（`use foo::*`）を避け、使う項目を明示的に取り込む。prelude など慣例的な例外を除く（ER Item 23）。
- 公開 API のシグネチャに他クレートの型が現れる場合、その型を re-export し、利用者がバージョンを合わせられるようにする（ER Item 24）。
- 依存グラフを管理する。`cargo tree` で重複を確認し、`cargo-deny`／`cargo audit` でライセンスと脆弱性を点検する（ER Item 25）。
- feature の乱用を避ける。feature は加算的（additive）に保ち、有効化で既存の挙動を壊さない（ER Item 26）。
- 仕様が巨大でバグリスクの高い処理（旧 Office `.doc`／`.ppt` のバイナリ手書き解析など）を自前実装する前に、既存クレートや外部ツール（`antiword`／`catdoc` 等）で要件を満たせないか調査し、難度に見合うか評価してから着手する（プロジェクト方針）。

```rust
// NG: 何が入るか不明瞭
use crate::models::*;
// OK: 取り込む項目を明示
use crate::models::{User, Account};
```

---

## §11 ドキュメントとマクロ

- すべての公開要素（型・関数・トレイト・モジュール）にドキュメントコメントを書く（ER Item 27）。
- ドキュメントは `///`、モジュール／クレート全体は `//!` を使う。実装上の補足は `//` を使う（Kernel）。
- ドキュメントの先頭は、その要素が何をするかを述べる一文にする。詳細は段落を分けて続ける（Kernel）。
- 使用例は `# Examples` セクションに、パニック条件は `# Panics`、`unsafe` 関数の前提は `# Safety` セクションに書く（Kernel）。
- コメントは文として大文字始まり・ピリオド終わりにする。`// SAFETY:`・`// TODO:`・`// FIXME:` などのタグ付きコメントも同様にする（Kernel）。
- 非公開要素にドキュメントを書く場合も `///` を使う（Kernel）。
- マクロは控えめに使う。関数・ジェネリクスで表現できるものをマクロにしない（ER Item 28）。
- 必要な場合はまず宣言的マクロ（`macro_rules!`）を検討し、手続き的マクロは正当な理由がある場合に限る（ER Item 28）。

```rust
/// 設定ファイルを読み込んで [`Config`] を返す。
///
/// # Examples
///
/// ```
/// let cfg = load_config("app.toml")?;
/// ```
///
/// # Errors
///
/// ファイルが存在しない、または TOML として不正な場合に [`LoadError`] を返す。
pub fn load_config(path: &str) -> Result<Config, LoadError> {
    // ...
}
```

---

## §12 ツーリング（Clippy・テスト・CI）

- `cargo clippy` を実行し、警告に従う。`#[allow(...)]` での抑制は理由をコメントで添える（ER Item 29）。
- ユニットテストだけでなく、ドキュメントテスト・結合テスト（`tests/`）も書く。境界値はプロパティテスト（`proptest`）やファジング（`cargo-fuzz`）で補う（ER Item 30）。
- ツールエコシステムを活用する（`rustfmt`・`clippy`・`rust-analyzer`・`miri`・`cargo-deny`）（ER Item 31）。
- CI を設定し、`fmt --check`・`clippy -D warnings`・`test` を最低限すべてのプッシュで実行する（ER Item 32）。

---

## §13 標準を超えて（no_std・FFI）

- 組込みや幅広い再利用を想定するライブラリは `no_std` 互換を検討する（ER Item 33）。
- FFI 境界を通る型・所有権・ライフタイムを明示的に制御し、境界の責務を最小化する（ER Item 34）。
- C との結合は手書きの FFI マッピングより `bindgen`（および `cbindgen`）で自動生成する（ER Item 35）。

---

## §14 リファクタリング・プレイブック（main を薄く保つ）

> 個々の設計判断（型・エラー・builder）の根拠は §1〜§13 の Effective Rust 項目に従う。本節はそれらを「`main` を薄く保ちロジックを lib へ寄せる」手順として束ねる（TRPL §12.3）。

- `main` 関数に複数の仕事（引数解析・設定構築・実行・エラー処理）を持たせない。各責務を関数に分離する（TRPL §12.3）。
- 関連する設定値は個別の引数で持ち回らず、設定用の `struct` にまとめる（TRPL §12.3）。
- 中核ロジックは `lib.rs`（ライブラリクレート）へ寄せ、`main.rs` は配線のみにする。これによりロジックを結合テストできる（TRPL §12.3）。
- 処理本体を `run` 関数に集約し、`run` は `Result` を返す。`main` は `run` の `Err` を受けて終了コードを制御する（TRPL §12.3）。
- エラーは `expect` のメッセージで済ませず、意味のあるエラー型で返す（根拠は §3 / ER Item 4, 18）。

```rust
// main.rs: 配線とエラー処理のみ
fn main() {
    let cfg = Config::build(env::args()).unwrap_or_else(|e| {
        eprintln!("引数の解析に失敗: {e}");
        process::exit(2);
    });
    if let Err(e) = myapp::run(cfg) {
        eprintln!("実行時エラー: {e}");
        process::exit(1);
    }
}
```

---

## §15 整形・レイアウト

- 整形は `rustfmt` のデフォルト設定に従い、整形ルールを個別に議論しない（Style）。
- インデントは空白 4 個。タブは使わない（Style）。
- 1 行は最大 100 文字（Style）。
- 折り返しはブロックインデントを使い、開き括弧位置に揃えるビジュアルインデントを避ける（Style）。
- 複数行に分割した要素の末尾にはトレイリングカンマを付ける（Style）。
- `use` は標準ライブラリ・外部クレート・自クレートの 3 群に分け、各群をアルファベット順に並べる（Style）。
- 命名は次に従う（Style）。
  - 型・トレイト・enum・enum 値：`UpperCamelCase`
  - 関数・メソッド・変数・モジュール：`snake_case`
  - 定数・`static`：`SCREAMING_SNAKE_CASE`
- コミット前に `cargo fmt` を実行し、整形差分をレビュー対象に混ぜない（Style）。

---

## §16 リファクタリング指針（関数型志向と最適化）

> 既定は関数型で書く。手続き的な最適化は、計測でホットスポットと判明した箇所に限る（ER Item 20）。出典：Martin Fowler『Refactoring』カタログ（<https://refactoring.com/catalog/>）、The Rust Performance Book（<https://nnethercote.github.io/perf-book/>）。

### 関数型で書く（明示的ループを避ける）

- `for`／`while` ループは、まずイテレータ変換（`map`・`filter`・`fold`・`collect`・`sum`・`any`・`all`）で置き換えられないか検討する（ER Item 9、Fowler "Replace Loop with Pipeline"）。
- `match` での分岐は、`Option`／`Result` の変換メソッド（`map`・`and_then`・`?`）で置き換えられないか検討する（ER Item 3）。
- 添字アクセスのループは避ける。Clippy `needless_range_loop` に従う（<https://rust-lang.github.io/rust-clippy/master/index.html#needless_range_loop>）。
- アキュムレータ変数を伴うループは `fold`／`sum`／`collect` で畳み込み、可変状態を消す。
- 副作用のみが目的の繰り返しに限り `for` を使う。

```rust
// NG: 明示ループ＋可変状態
let mut total = 0;
for o in &orders { if o.paid { total += o.amount; } }
// OK: パイプライン（ER Item 9 / Fowler Replace Loop with Pipeline）
let total: u64 = orders.iter().filter(|o| o.paid).map(|o| o.amount).sum();
```

### ネストは2段まで（早期リターン・関数抽出）

- `if`／`for`／`while` のネストは2段までにする。3段以上になったら早期リターンか関数抽出で平坦化する（Fowler "Replace Nested Conditional with Guard Clauses" / "Extract Function"）。
- ガード節で前提条件を先に弾き、本体のインデントを下げる。
- `let ... else` と `?` で早期脱出し、`else` のネストを消す。
- 認知的複雑度は Clippy `cognitive_complexity` で監視する。

```rust
// NG: ネスト3段
fn process(x: Option<&Item>) -> Result<u64, Error> {
    if let Some(i) = x { if i.valid { if i.amount > 0 { return Ok(i.amount); } } }
    Err(Error::Invalid)
}
// OK: ガード節で平坦化（let ... else / 早期リターン）
fn process(x: Option<&Item>) -> Result<u64, Error> {
    let Some(i) = x else { return Err(Error::Invalid) };
    if !i.valid || i.amount == 0 { return Err(Error::Invalid); }
    Ok(i.amount)
}
```

### ループ最適化（計測が前提）

- まず計測する。`cargo bench`（Criterion）やプロファイラで遅いと確認した箇所だけ最適化する（ER Item 20、Rust Performance Book）。
- ループ不変式（毎回同じ結果になる計算・参照解決）はループ外の局所変数に巻き上げる（loop-invariant code motion）。
- ループ内で繰り返し参照するフィールドやメソッド結果は、ループ前にローカル束縛してアクセスを減らす。
- 確保（allocation）をループ外に出す。バッファは `Vec::with_capacity` で事前確保して再利用する。
- Rust のイテレータはゼロコスト抽象であり、パイプライン化は多くの場合性能を落とさず境界チェックも省ける。関数型への書き換えと最適化は対立しないことが多い（確信度：高、Rust Performance Book）。

```rust
// NG: ループ不変な参照解決を毎回行う
for row in &rows { out.push(row.value * config.rate.factor()); }
// OK: 不変式を巻き上げ、容量を事前確保
let factor = config.rate.factor();              // ループ不変式の巻き上げ
let mut out = Vec::with_capacity(rows.len());   // 確保をループ外へ
for row in &rows { out.push(row.value * factor); }
```

### 事前確保（capacity reservation）

> 要素数が事前に分かるなら容量を確保し、成長中の再確保とコピーを避ける。Rust では有効だが、効くのは手続きループに限る。出典：The Rust Performance Book（<https://nnethercote.github.io/perf-book/heap-allocations.html>）、std `Iterator::size_hint`／`FromIterator`（<https://doc.rust-lang.org/std/iter/trait.Iterator.html#method.size_hint>）。

- `for` ＋ `push` で件数が分かるコレクションは事前確保する：`Vec::with_capacity(n)`・`String::with_capacity(n)`・`HashMap::with_capacity(n)`・`HashSet::with_capacity(n)`。再確保（容量倍増とムーブ）の回数を 0 にできる。
- 既存コレクションへ一括追加する前は `reserve(n)`／`reserve_exact(n)` で追加分をまとめて確保する。
- **イテレータ変換では手動確保は不要**：`collect()`／`extend()` は `size_hint()` を見て自動で事前確保する。`ExactSizeIterator`（`Vec`・`Range`・`slice::iter` など由来）は容量が正確に取られるため、`with_capacity` を書いてから `collect` するのは冗長。手動確保は §16 のパイプライン化で消せない手続きループに限定する。
- ヒープ確保自体を避ける：固定長で済むなら `[T; N]`（スタック）、件数が小さく可変なら `smallvec`／`arrayvec` でスタック上に確保する。
- リスク・注意点：効くのは実際に約 n 件埋める場合のみ。過大見積りはメモリの浪費、過小なら結局再確保が走る。`HashMap`／`HashSet` の容量はロードファクタ込みの近似（n 件を resize なしで収められる量を確保する、という意味）。確信度：高だが、適用は §16 冒頭どおり計測でホットスポットと判明した箇所に限る。

```rust
// NG: with_capacity してから collect（size_hint で二重に確保され冗長）
let mut out = Vec::with_capacity(rows.len());
out = rows.iter().map(|r| r.value).collect();   // collect 側が再度確保する
// OK-1: イテレータ変換は collect に任せる（ExactSizeIterator が容量を正確化）
let out: Vec<u64> = rows.iter().map(|r| r.value).collect();
// OK-2: パイプライン化できない手続きループは手動で事前確保する
let mut acc = Vec::with_capacity(rows.len());
for r in &rows { if r.valid { acc.push(expand(r)); } }   // 件数の上限が分かる
```
