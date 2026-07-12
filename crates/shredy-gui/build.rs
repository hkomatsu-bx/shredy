//! `.slint` ビューをコンパイルし、Windows では exe リソースへアプリアイコンを埋め込む（設計 §2.1）。

fn main() {
    let config = slint_build::CompilerConfiguration::new().with_style("fluent".into());
    slint_build::compile_with_config("ui/app.slint", config)
        .expect("Slint ビューのコンパイルに失敗しました");

    embed_windows_icon();
}

/// Windows: アプリアイコン（icon.ico）を exe リソースへ埋め込む。
/// タスクバー・タイトルバー・エクスプローラの表示に用いられる。
#[cfg(windows)]
fn embed_windows_icon() {
    const ICON: &str = "ui/icons/icon.ico";
    println!("cargo:rerun-if-changed={ICON}");
    let mut res = winresource::WindowsResource::new();
    res.set_icon(ICON);
    if let Err(e) = res.compile() {
        // アイコン埋め込み失敗はビルドを止めない（実行は可能・表示のみ既定アイコンになる）。
        println!("cargo:warning=アプリアイコンの埋め込みに失敗しました: {e}");
    }
}

#[cfg(not(windows))]
fn embed_windows_icon() {}
