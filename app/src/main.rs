//! HotRun: ホットキーランチャーの設定画面とトレイ常駐。
//! 設定は JSON（`%APPDATA%\hotrun\config.json`）。CLI の `hotrun` と同じファイルを使う。

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

// 画面なしのビルド（ロジックのテスト用）では使われない部分がある
#![cfg_attr(not(feature = "gui"), allow(dead_code, unused_imports))]

mod engine;
mod store;

#[cfg(feature = "gui")]
mod gui;

#[cfg(feature = "gui")]
fn main() {
    gui::run();
}

#[cfg(not(feature = "gui"))]
fn main() {
    eprintln!("画面なしでビルドされています（--no-default-features）");
    std::process::exit(2);
}
