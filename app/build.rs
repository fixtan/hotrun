// 画面(Tauri)を含むビルドのときだけ、Tauri のビルド処理を走らせる。
// Windows では icons/icon.ico を exe に埋め込む。
fn main() {
    #[cfg(feature = "gui")]
    tauri_build::build();
}
