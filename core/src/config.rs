//! 設定（JSON）の型と検証。

use crate::hotkey::Hotkey;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// プログラムを実行する（引数・作業ディレクトリ指定可）
    #[default]
    Run,
    /// ファイルを関連付けで開く
    OpenFile,
    /// フォルダを開く
    OpenFolder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Window {
    #[default]
    Normal,
    Maximized,
    Minimized,
    Hidden,
}

fn yes() -> bool {
    true
}

fn is_yes(b: &bool) -> bool {
    *b
}

#[allow(clippy::ptr_arg)] // serde の skip_serializing_if は &String を要求する
fn is_empty(s: &String) -> bool {
    s.is_empty()
}

fn is_default<T: Default + PartialEq>(v: &T) -> bool {
    *v == T::default()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    pub hotkey: Hotkey,
    #[serde(default, skip_serializing_if = "is_default")]
    pub kind: Kind,
    pub target: String,
    #[serde(default, skip_serializing_if = "is_empty")]
    pub args: String,
    #[serde(default, skip_serializing_if = "is_empty")]
    pub cwd: String,
    #[serde(default, skip_serializing_if = "is_default")]
    pub window: Window,
    #[serde(default, skip_serializing_if = "is_empty")]
    pub label: String,
    #[serde(default, skip_serializing_if = "is_empty")]
    pub group: String,
    #[serde(default = "yes", skip_serializing_if = "is_yes")]
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    pub version: u32,
    #[serde(default)]
    pub entries: Vec<Entry>,
}

impl Default for Config {
    fn default() -> Self {
        Config { version: 1, entries: vec![] }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Issue {
    /// 同じホットキーが複数の項目に割り当てられている
    DuplicateHotkey { hotkey: Hotkey, ids: Vec<String> },
    DuplicateId { id: String },
    EmptyTarget { id: String },
    /// Windows が予約していて登録できない組み合わせ
    Reserved { id: String, hotkey: Hotkey, why: &'static str },
    /// 修飾キーなしの文字・数字キー（入力を奪ってしまう）
    BareTypingKey { id: String, hotkey: Hotkey },
}

impl std::fmt::Display for Issue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Issue::DuplicateHotkey { hotkey, ids } => {
                write!(f, "{hotkey} が重複しています: {}", ids.join(", "))
            }
            Issue::DuplicateId { id } => write!(f, "id「{id}」が重複しています"),
            Issue::EmptyTarget { id } => write!(f, "{id}: 実行対象が空です"),
            Issue::Reserved { id, hotkey, why } => {
                write!(f, "{id}: {hotkey} は登録できません（{why}）")
            }
            Issue::BareTypingKey { id, hotkey } => {
                write!(f, "{id}: {hotkey} は修飾キーなしの文字キーで、入力を奪います")
            }
        }
    }
}

#[derive(Debug)]
pub enum LoadError {
    Io(std::io::Error),
    Parse(serde_json::Error),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::Io(e) => write!(f, "読み書きに失敗: {e}"),
            LoadError::Parse(e) => write!(f, "JSON の形式が正しくありません: {e}"),
        }
    }
}

impl std::error::Error for LoadError {}

impl Config {
    pub fn load(path: &Path) -> Result<Config, LoadError> {
        let text = std::fs::read_to_string(path).map_err(LoadError::Io)?;
        serde_json::from_str(&text).map_err(LoadError::Parse)
    }

    pub fn save(&self, path: &Path) -> Result<(), LoadError> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(LoadError::Io)?;
        }
        let text = serde_json::to_string_pretty(self).map_err(LoadError::Parse)?;
        // 途中で壊れないよう、一時ファイルに書いてから置き換える
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, text).map_err(LoadError::Io)?;
        std::fs::rename(&tmp, path).map_err(LoadError::Io)
    }

    /// 有効な項目のみを対象に問題を洗い出す
    pub fn check(&self) -> Vec<Issue> {
        let mut issues = vec![];

        let mut seen_ids: HashMap<&str, usize> = HashMap::new();
        for e in &self.entries {
            *seen_ids.entry(&e.id).or_default() += 1;
        }
        let mut dup_ids: Vec<&str> = seen_ids.iter().filter(|(_, n)| **n > 1).map(|(k, _)| *k).collect();
        dup_ids.sort();
        for id in dup_ids {
            issues.push(Issue::DuplicateId { id: id.to_string() });
        }

        let mut by_key: Vec<(Hotkey, Vec<String>)> = vec![];
        for e in self.entries.iter().filter(|e| e.enabled) {
            if e.target.trim().is_empty() {
                issues.push(Issue::EmptyTarget { id: e.id.clone() });
            }
            if let Some(why) = reserved(&e.hotkey) {
                issues.push(Issue::Reserved { id: e.id.clone(), hotkey: e.hotkey, why });
            }
            if e.hotkey.is_bare() && is_typing_key(e.hotkey.vk) {
                issues.push(Issue::BareTypingKey { id: e.id.clone(), hotkey: e.hotkey });
            }
            match by_key.iter_mut().find(|(k, _)| *k == e.hotkey) {
                Some((_, ids)) => ids.push(e.id.clone()),
                None => by_key.push((e.hotkey, vec![e.id.clone()])),
            }
        }
        for (hotkey, ids) in by_key {
            if ids.len() > 1 {
                issues.push(Issue::DuplicateHotkey { hotkey, ids });
            }
        }
        issues
    }
}

fn is_typing_key(vk: u32) -> bool {
    matches!(vk, 0x30..=0x39 | 0x41..=0x5A | 32 | 186..=192 | 219..=222)
}

/// 登録できないことが確実なものだけを挙げる（外部アプリとの衝突は実際に登録して初めて分かる）
fn reserved(h: &Hotkey) -> Option<&'static str> {
    use crate::hotkey::{ALT, CTRL, SHIFT, WIN};
    if h.mods == CTRL | ALT && h.vk == 46 {
        return Some("Ctrl+Alt+Delete は Windows 専用");
    }
    if h.mods == WIN && h.vk == 0x4C {
        return Some("Win+L は Windows が画面ロックに使用");
    }
    if h.mods == WIN | SHIFT && h.vk == 0x4C {
        return Some("Win+Shift+L は Windows が使用");
    }
    if h.mods == ALT && h.vk == 115 {
        return Some("Alt+F4 は Windows がウィンドウを閉じるのに使用");
    }
    if h.mods == WIN && h.vk == 0x44 {
        return Some("Win+D は Windows がデスクトップ表示に使用");
    }
    None
}

/// 既定の設定ファイルの場所（Windows は %APPDATA%\hotrun\config.json）
pub fn default_path() -> std::path::PathBuf {
    use std::path::PathBuf;
    if let Some(d) = std::env::var_os("APPDATA") {
        return PathBuf::from(d).join("hotrun").join("config.json");
    }
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    home.join(".config").join("hotrun").join("config.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, hk: &str) -> Entry {
        Entry {
            id: id.into(),
            hotkey: Hotkey::parse(hk).unwrap(),
            kind: Kind::Run,
            target: "notepad.exe".into(),
            args: String::new(),
            cwd: String::new(),
            window: Window::Normal,
            label: String::new(),
            group: String::new(),
            enabled: true,
        }
    }

    #[test]
    fn detects_duplicates_and_reserved() {
        let cfg = Config {
            version: 1,
            entries: vec![
                entry("a", "Ctrl+Shift+1"),
                entry("b", "ctrl+shift+1"),
                entry("c", "Win+L"),
                entry("d", "A"),
                entry("e", "F19"),
            ],
        };
        let issues = cfg.check();
        assert!(issues.iter().any(|i| matches!(i, Issue::DuplicateHotkey { ids, .. } if ids == &["a", "b"])));
        assert!(issues.iter().any(|i| matches!(i, Issue::Reserved { id, .. } if id == "c")));
        assert!(issues.iter().any(|i| matches!(i, Issue::BareTypingKey { id, .. } if id == "d")));
        assert!(!issues.iter().any(|i| matches!(i, Issue::BareTypingKey { id, .. } if id == "e")));
    }

    #[test]
    fn disabled_entries_are_ignored() {
        let mut b = entry("b", "Ctrl+Shift+1");
        b.enabled = false;
        let cfg = Config { version: 1, entries: vec![entry("a", "Ctrl+Shift+1"), b] };
        assert!(cfg.check().is_empty());
    }

    #[test]
    fn json_roundtrip_is_compact() {
        let cfg = Config { version: 1, entries: vec![entry("a", "Ctrl+Shift+1")] };
        let text = serde_json::to_string(&cfg).unwrap();
        assert!(!text.contains("args"), "空の項目は書き出さない: {text}");
        let back: Config = serde_json::from_str(&text).unwrap();
        assert_eq!(back, cfg);
    }
}
