//! 他のアプリがすでに使っているホットキーの洗い出し。
//! 全組み合わせに RegisterHotKey を試し、「すでに使用中」で断られたものを集める（Windows 側は win.rs の `scan_taken`）。
//! ここでは、試す候補の列挙と、結果の分類・並べ替えだけを扱う（OS に依存しないのでテストできる）。

use crate::hotkey::{Hotkey, ALT, CTRL, SHIFT, WIN};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Class {
    /// HotRun 自身が取っているキー
    #[serde(rename = "hotrun")]
    HotRun,
    /// Win キーを含む組み合わせ。Windows 標準、または PowerToys など Win キーを使うアプリ
    Win,
    /// それ以外。どのアプリかは Windows からは分からない
    Other,
}

#[derive(Debug, Clone, Serialize)]
pub struct Taken {
    pub hotkey: String,
    pub mods: u32,
    pub vk: u32,
    pub class: Class,
}

/// 試すキー（仮想キーコード）。修飾キーそのものや、マウスボタン、IME 用のキーは除く。
pub fn candidate_vks() -> Vec<u32> {
    let mut v = vec![8, 9, 13, 19, 20, 27, 32];
    v.extend(33..=40); // PageUp, PageDown, End, Home, 矢印
    v.extend(44..=46); // PrintScreen, Insert, Delete
    v.extend(48..=57); // 0-9
    v.extend(65..=90); // A-Z
    v.extend(96..=107); // テンキー 0-9, *, +
    v.extend(109..=135); // テンキー - . /、F1-F24
    v.extend([144, 145]); // NumLock, ScrollLock
    v.extend(166..=183); // ブラウザ・音量・メディアキー
    v.extend(186..=192);
    v.extend(219..=222);
    v
}

/// 試す組み合わせを全部（修飾キー 16 通り × キー）
pub fn candidates() -> Vec<Hotkey> {
    let vks = candidate_vks();
    let mut out = Vec::with_capacity(16 * vks.len());
    for mods in 0..16u32 {
        debug_assert!(mods & !(ALT | CTRL | SHIFT | WIN) == 0);
        for &vk in &vks {
            out.push(Hotkey { mods, vk });
        }
    }
    out
}

pub fn classify(h: &Hotkey, own: &[Hotkey]) -> Class {
    if own.contains(h) {
        Class::HotRun
    } else if h.mods & WIN != 0 {
        Class::Win
    } else {
        Class::Other
    }
}

/// 使用中のキーを分類して、分類ごと→キーの順に並べる
pub fn build(taken: &[Hotkey], own: &[Hotkey]) -> Vec<Taken> {
    let mut v: Vec<Taken> = taken
        .iter()
        .map(|h| Taken { hotkey: h.to_string(), mods: h.mods, vk: h.vk, class: classify(h, own) })
        .collect();
    v.sort_by_key(|t| (t.class, t.mods, t.vk));
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hk(s: &str) -> Hotkey {
        Hotkey::parse(s).unwrap()
    }

    #[test]
    fn candidates_cover_all_modifier_combinations() {
        let c = candidates();
        assert_eq!(c.len(), 16 * candidate_vks().len());
        assert!(c.contains(&hk("Ctrl+Shift+M")));
        assert!(c.contains(&hk("Ctrl+Alt+Shift+Win+F24")));
        assert!(c.contains(&hk("F19")));
        assert!(c.contains(&hk("VolumeMute")));
        assert!(c.contains(&hk("Win+E")));
    }

    #[test]
    fn candidates_exclude_modifier_and_mouse_keys() {
        let v = candidate_vks();
        for bad in [1, 2, 4, 16, 17, 18, 91, 92, 93, 160, 161, 162, 163, 164, 165] {
            assert!(!v.contains(&bad), "VK {bad}");
        }
        let mut sorted = v.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), v.len(), "重複なし");
    }

    #[test]
    fn every_candidate_has_a_name() {
        // 一覧に「VK173」のような名前で出ないこと
        for vk in candidate_vks() {
            if matches!(vk, 20 | 144 | 145) {
                continue; // CapsLock / NumLock / ScrollLock は名前を持たない（普通は使わない）
            }
            assert!(crate::hotkey::key_name(vk).is_some(), "VK {vk} に名前がない");
        }
    }

    #[test]
    fn classify_own_then_win_then_other() {
        let own = [hk("Ctrl+1")];
        assert_eq!(classify(&hk("Ctrl+1"), &own), Class::HotRun);
        assert_eq!(classify(&hk("Win+E"), &own), Class::Win);
        assert_eq!(classify(&hk("Ctrl+Shift+M"), &own), Class::Other);
        assert_eq!(classify(&hk("Ctrl+Win+Shift+M"), &own), Class::Win);
    }

    #[test]
    fn class_names_match_the_ui() {
        // scan.js が見ている名前（snake_case のままだと "hot_run" になって一覧に出なかった）
        let j = |c: Class| serde_json::to_string(&c).unwrap();
        assert_eq!(j(Class::HotRun), "\"hotrun\"");
        assert_eq!(j(Class::Win), "\"win\"");
        assert_eq!(j(Class::Other), "\"other\"");
    }

    #[test]
    fn build_sorts_by_class_then_key() {
        let taken = [hk("Ctrl+Shift+M"), hk("Win+E"), hk("Ctrl+1"), hk("Alt+F1")];
        let r = build(&taken, &[hk("Ctrl+1")]);
        let order: Vec<(&str, Class)> = r.iter().map(|t| (t.hotkey.as_str(), t.class)).collect();
        assert_eq!(
            order,
            vec![("Ctrl+1", Class::HotRun), ("Win+E", Class::Win), ("Alt+F1", Class::Other), ("Ctrl+Shift+M", Class::Other)]
        );
    }
}
