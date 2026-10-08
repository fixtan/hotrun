//! ホットキーの表現。修飾キーのビットは Win32 の `RegisterHotKey` と同じ。

use serde::{Deserialize, Serialize};
use std::fmt;

pub const ALT: u32 = 1;
pub const CTRL: u32 = 2;
pub const SHIFT: u32 = 4;
pub const WIN: u32 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Hotkey {
    pub mods: u32,
    pub vk: u32,
}

/// 仮想キーコードと表示名の対応（名前は大文字小文字を区別せず解釈する）
const NAMED_KEYS: &[(&str, u32)] = &[
    ("Backspace", 8),
    ("Tab", 9),
    ("Enter", 13),
    ("Pause", 19),
    ("Esc", 27),
    ("Space", 32),
    ("PageUp", 33),
    ("PageDown", 34),
    ("End", 35),
    ("Home", 36),
    ("Left", 37),
    ("Up", 38),
    ("Right", 39),
    ("Down", 40),
    ("PrintScreen", 44),
    ("Insert", 45),
    ("Delete", 46),
    ("Num0", 96),
    ("Num1", 97),
    ("Num2", 98),
    ("Num3", 99),
    ("Num4", 100),
    ("Num5", 101),
    ("Num6", 102),
    ("Num7", 103),
    ("Num8", 104),
    ("Num9", 105),
    ("Num*", 106),
    ("Num+", 107),
    ("Num-", 109),
    ("Num.", 110),
    ("Num/", 111),
    (";", 186),
    ("=", 187),
    (",", 188),
    ("-", 189),
    (".", 190),
    ("/", 191),
    ("`", 192),
    ("[", 219),
    ("\\", 220),
    ("]", 221),
    ("'", 222),
];

pub fn key_name(vk: u32) -> Option<String> {
    match vk {
        0x30..=0x39 | 0x41..=0x5A => Some((vk as u8 as char).to_string()),
        0x70..=0x87 => Some(format!("F{}", vk - 0x70 + 1)),
        _ => NAMED_KEYS
            .iter()
            .find(|(_, v)| *v == vk)
            .map(|(n, _)| (*n).to_string()),
    }
}

pub fn key_vk(name: &str) -> Option<u32> {
    let n = name.trim();
    if n.chars().count() == 1 {
        let c = n.chars().next()?.to_ascii_uppercase();
        if c.is_ascii_digit() || c.is_ascii_uppercase() {
            return Some(c as u32);
        }
    }
    let up = n.to_ascii_uppercase();
    if let Some(num) = up.strip_prefix('F') {
        if let Ok(k) = num.parse::<u32>() {
            if (1..=24).contains(&k) {
                return Some(0x70 + k - 1);
            }
        }
    }
    NAMED_KEYS
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(n))
        .map(|(_, v)| *v)
}

impl Hotkey {
    pub fn parse(s: &str) -> Result<Hotkey, String> {
        let mut mods = 0;
        let mut key: Option<u32> = None;
        // 「+」自体をキー名にできるよう、分割後に空要素が出たら "+" とみなす…のは曖昧なので
        // 「Num+」を使う方針にして、ここでは単純に分割する
        for part in s.split('+') {
            let p = part.trim();
            if p.is_empty() {
                return Err(format!("ホットキー「{s}」を読み取れません"));
            }
            match p.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => mods |= CTRL,
                "shift" => mods |= SHIFT,
                "alt" => mods |= ALT,
                "win" | "windows" | "super" => mods |= WIN,
                _ => {
                    let vk = key_vk(p).ok_or_else(|| format!("未知のキー名「{p}」"))?;
                    if key.replace(vk).is_some() {
                        return Err(format!("ホットキー「{s}」にキーが2つあります"));
                    }
                }
            }
        }
        let vk = key.ok_or_else(|| format!("ホットキー「{s}」にキーがありません"))?;
        Ok(Hotkey { mods, vk })
    }

    /// 修飾キーなしのホットキーか（F19 など。通常の文字キーだと入力を奪うので注意が必要）
    pub fn is_bare(&self) -> bool {
        self.mods == 0
    }
}

impl fmt::Display for Hotkey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.mods & CTRL != 0 {
            write!(f, "Ctrl+")?;
        }
        if self.mods & ALT != 0 {
            write!(f, "Alt+")?;
        }
        if self.mods & SHIFT != 0 {
            write!(f, "Shift+")?;
        }
        if self.mods & WIN != 0 {
            write!(f, "Win+")?;
        }
        match key_name(self.vk) {
            Some(n) => write!(f, "{n}"),
            None => write!(f, "VK{}", self.vk),
        }
    }
}

impl TryFrom<String> for Hotkey {
    type Error = String;
    fn try_from(s: String) -> Result<Self, String> {
        // 未知の VK は "VK123" 形式で保存・復元できるようにする
        if let Some(rest) = s.strip_prefix("VK") {
            if let Ok(vk) = rest.parse::<u32>() {
                return Ok(Hotkey { mods: 0, vk });
            }
        }
        if let Some((head, tail)) = s.rsplit_once('+') {
            if let Some(rest) = tail.strip_prefix("VK") {
                if let Ok(vk) = rest.parse::<u32>() {
                    let base = Hotkey::parse(&format!("{head}+A"))?;
                    return Ok(Hotkey { mods: base.mods, vk });
                }
            }
        }
        Hotkey::parse(&s)
    }
}

impl From<Hotkey> for String {
    fn from(h: Hotkey) -> String {
        h.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_display_roundtrip() {
        let h = Hotkey::parse("shift + ctrl + 1").unwrap();
        assert_eq!(h, Hotkey { mods: CTRL | SHIFT, vk: 49 });
        assert_eq!(h.to_string(), "Ctrl+Shift+1");
        assert_eq!(Hotkey::parse("F19").unwrap(), Hotkey { mods: 0, vk: 130 });
        assert_eq!(Hotkey::parse("Ctrl+[").unwrap().vk, 219);
        assert_eq!(Hotkey::parse("Ctrl+Left").unwrap().vk, 37);
    }

    #[test]
    fn rejects_bad_input() {
        assert!(Hotkey::parse("Ctrl+Shift").is_err());
        assert!(Hotkey::parse("A+B").is_err());
        assert!(Hotkey::parse("Ctrl+Foo").is_err());
        assert!(Hotkey::parse("").is_err());
    }

    #[test]
    fn unknown_vk_survives_roundtrip() {
        let h = Hotkey { mods: CTRL, vk: 250 };
        let s: String = h.into();
        assert_eq!(s, "Ctrl+VK250");
        assert_eq!(Hotkey::try_from(s).unwrap(), h);
    }
}
