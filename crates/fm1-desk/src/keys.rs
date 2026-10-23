//! The computer keyboard as a music keyboard, in two layouts, for both boards. One table per
//! layout: the input handling and the letters drawn on the panel's keys both read it.
//!
//! - Ableton (fm1-live's and the web page's): A S D F G H J K L ; ' the white keys from C4,
//!   W E T Y U O P the black ones; Z / X octave, Space PLAY, R REC, 1..0 the FM-1's buttons /
//!   1..8 the SMK-37's top pads; arrows and , . the first three encoders.
//! - FL Studio / tracker: two rows, Z S X D C V G B H N J M , L . ; / (from C) and Q 2 W 3 E R 5
//!   T 6 Y 7 U I 9 O 0 P (an octave up); the controls move off the notes: - / = octave,
//!   Space PLAY, Enter REC, F3..F12 the FM-1's buttons / F3..F10 the SMK-37's pads; arrows and
//!   Page Up / Down the encoders.

use eframe::egui::Key;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, Default)]
pub enum Layout {
    #[default]
    Ableton,
    Tracker,
}

impl Layout {
    pub fn name(self) -> &'static str {
        match self {
            Layout::Ableton => "Ableton",
            Layout::Tracker => "FL / tracker",
        }
    }
}

/// (key, semitones above the layout's lowest C, its letter)
const ABLETON: [(Key, i32, &str); 18] = [
    (Key::A, 0, "A"), (Key::W, 1, "W"), (Key::S, 2, "S"), (Key::E, 3, "E"), (Key::D, 4, "D"), (Key::F, 5, "F"),
    (Key::T, 6, "T"), (Key::G, 7, "G"), (Key::Y, 8, "Y"), (Key::H, 9, "H"), (Key::U, 10, "U"), (Key::J, 11, "J"),
    (Key::K, 12, "K"), (Key::O, 13, "O"), (Key::L, 14, "L"), (Key::P, 15, "P"), (Key::Semicolon, 16, ";"), (Key::Quote, 17, "'"),
];

const TRACKER: [(Key, i32, &str); 34] = [
    (Key::Z, 0, "Z"), (Key::S, 1, "S"), (Key::X, 2, "X"), (Key::D, 3, "D"), (Key::C, 4, "C"), (Key::V, 5, "V"),
    (Key::G, 6, "G"), (Key::B, 7, "B"), (Key::H, 8, "H"), (Key::N, 9, "N"), (Key::J, 10, "J"), (Key::M, 11, "M"),
    (Key::Comma, 12, ","), (Key::L, 13, "L"), (Key::Period, 14, "."), (Key::Semicolon, 15, ";"), (Key::Slash, 16, "/"),
    (Key::Q, 12, "Q"), (Key::Num2, 13, "2"), (Key::W, 14, "W"), (Key::Num3, 15, "3"), (Key::E, 16, "E"), (Key::R, 17, "R"),
    (Key::Num5, 18, "5"), (Key::T, 19, "T"), (Key::Num6, 20, "6"), (Key::Y, 21, "Y"), (Key::Num7, 22, "7"), (Key::U, 23, "U"),
    (Key::I, 24, "I"), (Key::Num9, 25, "9"), (Key::O, 26, "O"), (Key::Num0, 27, "0"), (Key::P, 28, "P"),
];

fn table(l: Layout) -> &'static [(Key, i32, &'static str)] {
    match l {
        Layout::Ableton => &ABLETON,
        Layout::Tracker => &TRACKER,
    }
}

/// The FM-1's note keys are matrix ids 14..=40 (F3..G5, C4 = 21); the layout's lowest C is C4.
const FM1_C: i32 = 21;
/// The SMK-37's keys 0..=36 are C3..C6: Ableton's row starts at C4 (12), the tracker's at C3.
fn smk_c(l: Layout) -> i32 {
    match l {
        Layout::Ableton => 12,
        Layout::Tracker => 0,
    }
}

/// The FM-1 key (matrix id) a computer key plays.
pub fn fm1_note(l: Layout, k: Key) -> Option<u8> {
    let s = table(l).iter().find(|e| e.0 == k)?.1;
    let id = FM1_C + s;
    (14..=40).contains(&id).then_some(id as u8)
}

/// The SMK-37 key (0..=36) a computer key plays.
pub fn smk_note(l: Layout, k: Key) -> Option<u8> {
    let s = table(l).iter().find(|e| e.0 == k)?.1;
    let id = smk_c(l) + s;
    (0..=36).contains(&id).then_some(id as u8)
}

/// The FM-1 button (matrix id 0..13: OCT- OCT+, FX SCL ENV LFO EDIT GLO HOME SAVE ARP SEQ, PLAY, REC).
pub fn fm1_button(l: Layout, k: Key) -> Option<u8> {
    let row = [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5, Key::Num6, Key::Num7, Key::Num8, Key::Num9, Key::Num0];
    let fs = [Key::F3, Key::F4, Key::F5, Key::F6, Key::F7, Key::F8, Key::F9, Key::F10, Key::F11, Key::F12];
    let (oct_dn, oct_up, rec, func) = match l {
        Layout::Ableton => (Key::Z, Key::X, Key::R, row),
        Layout::Tracker => (Key::Minus, Key::Equals, Key::Enter, fs),
    };
    if k == oct_dn {
        return Some(0);
    }
    if k == oct_up {
        return Some(1);
    }
    if k == Key::Space {
        return Some(12);
    }
    if k == rec {
        return Some(13);
    }
    func.iter().position(|&f| f == k).map(|i| 2 + i as u8)
}

/// The SMK-37 button (offset from `smk::BTN0`): 4 OCTAVE-, 5 OCTAVE+, 16 SEQ PLAY, 17 SEQ REC.
pub fn smk_button(l: Layout, k: Key) -> Option<u8> {
    match (l, k) {
        (Layout::Ableton, Key::Z) | (Layout::Tracker, Key::Minus) => Some(4),
        (Layout::Ableton, Key::X) | (Layout::Tracker, Key::Equals) => Some(5),
        (_, Key::Space) => Some(16),
        (Layout::Ableton, Key::R) | (Layout::Tracker, Key::Enter) => Some(17),
        _ => None,
    }
}

/// The SMK-37 top-row pad (0..7) a computer key presses.
pub fn smk_pad(l: Layout, k: Key) -> Option<usize> {
    let keys = match l {
        Layout::Ableton => [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5, Key::Num6, Key::Num7, Key::Num8],
        Layout::Tracker => [Key::F3, Key::F4, Key::F5, Key::F6, Key::F7, Key::F8, Key::F9, Key::F10],
    };
    keys.iter().position(|&p| p == k)
}

/// Encoder turns: (encoder 0..2, -1 / +1).
pub fn encoder(l: Layout, k: Key) -> Option<(usize, i32)> {
    match (l, k) {
        (_, Key::ArrowLeft) => Some((0, -1)),
        (_, Key::ArrowRight) => Some((0, 1)),
        (_, Key::ArrowDown) => Some((1, -1)),
        (_, Key::ArrowUp) => Some((1, 1)),
        (Layout::Ableton, Key::Comma) | (Layout::Tracker, Key::PageDown) => Some((2, -1)),
        (Layout::Ableton, Key::Period) | (Layout::Tracker, Key::PageUp) => Some((2, 1)),
        _ => None,
    }
}

/// The letters on a note key (FM-1 matrix id / SMK-37 key), as the layout plays it.
pub fn label(l: Layout, smk: bool, id: u8) -> String {
    let c = if smk { smk_c(l) } else { FM1_C };
    let s = id as i32 - c;
    let v: Vec<&str> = table(l).iter().filter(|e| e.1 == s).map(|e| e.2).collect();
    v.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layouts_map_both_boards() {
        // Ableton: A = C4 on both
        assert_eq!(fm1_note(Layout::Ableton, Key::A), Some(21));
        assert_eq!(smk_note(Layout::Ableton, Key::A), Some(12));
        assert_eq!(fm1_note(Layout::Ableton, Key::Quote), Some(38));
        // tracker: Z = C4 on the FM-1 (its lowest C), C3 on the SMK-37; Q an octave up
        assert_eq!(fm1_note(Layout::Tracker, Key::Z), Some(21));
        assert_eq!(fm1_note(Layout::Tracker, Key::Q), Some(33));
        assert_eq!(fm1_note(Layout::Tracker, Key::P), None); // (E6: above the FM-1's G5)
        assert_eq!(smk_note(Layout::Tracker, Key::Z), Some(0));
        assert_eq!(smk_note(Layout::Tracker, Key::P), Some(28));
        // the controls move off the notes in the tracker layout
        assert_eq!(fm1_button(Layout::Tracker, Key::Z), None);
        assert_eq!(fm1_button(Layout::Tracker, Key::Minus), Some(0));
        assert_eq!(fm1_button(Layout::Tracker, Key::F12), Some(11));
        assert_eq!(fm1_button(Layout::Ableton, Key::Num0), Some(11));
        assert_eq!(smk_pad(Layout::Tracker, Key::Num2), None);
        assert_eq!(encoder(Layout::Tracker, Key::Comma), None);
        // no computer key does two things in one layout
        for l in [Layout::Ableton, Layout::Tracker] {
            for &(k, _, _) in table(l) {
                assert!(fm1_button(l, k).is_none() && smk_button(l, k).is_none() && smk_pad(l, k).is_none() && encoder(l, k).is_none(), "{l:?} {k:?}");
            }
        }
        assert_eq!(label(Layout::Tracker, false, 33), ", Q");
        assert_eq!(label(Layout::Ableton, true, 13), "W");
    }
}
