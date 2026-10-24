//! The panels in colours: the real units' colourways (black, graphite, orange, mint, cream:
//! M-VAVE sells the FM-1 in them), and the FM-1's keybed as a piano keyboard in its groove
//! instead of the pills.
//!
//! "Classic" is upstream's own panel (fm1-live's panel.rs / panel_smk.rs, drawn by their own
//! `draw`): it follows upstream's changes. The others are those same drawings, the same shapes
//! on the same coordinates with upstream's drawing helpers, in a theme's colours; the SMK-37's
//! private helpers (its knob, wheel, labels) are copied here from panel_smk.rs as they are.

use crate::panel::{self, canvas, disc, mix, put, rrect, text, AMBER, ROOM};
use crate::panel_smk;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, Default, Hash)]
pub enum Theme {
    #[default]
    Classic,
    Black,
    Graphite,
    Orange,
    Mint,
    Cream,
}

impl Theme {
    pub const ALL: [Theme; 6] = [Theme::Classic, Theme::Black, Theme::Graphite, Theme::Orange, Theme::Mint, Theme::Cream];
    pub fn name(self) -> &'static str {
        match self {
            Theme::Classic => "Classic (the emulator's)",
            Theme::Black => "Black",
            Theme::Graphite => "Graphite",
            Theme::Orange => "Orange",
            Theme::Mint => "Mint",
            Theme::Cream => "Cream",
        }
    }
    pub fn short(self) -> &'static str {
        match self {
            Theme::Classic => "Classic",
            other => other.name(),
        }
    }
    /// A swatch for the menu: body, and the keys / caps.
    pub fn swatch(self) -> (u32, u32) {
        let p = fm1(self);
        (p.body_top, p.key_top)
    }
}

/// The FM-1's keybed: the unit's pill keys, or a piano keyboard in the same groove.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, Default, Hash)]
pub enum Keybed {
    #[default]
    Pills,
    Piano,
}

// ------------------------------------------------------------------ FM-1

/// The FM-1's colours (upstream's constants in panel.rs, as a theme).
struct Fm1 {
    body_top: u32,
    body_bot: u32,
    /// the OCT and button trays
    tray: u32,
    /// the keybed's groove
    well_top: u32,
    well_bot: u32,
    silk: u32,
    btn_top: u32,
    btn_bot: u32,
    btn_ink: u32,
    key_top: u32,
    key_bot: u32,
    key_line: u32,
    /// shadows under caps and keys
    shade: u32,
}

fn fm1(t: Theme) -> Fm1 {
    match t {
        Theme::Classic => Fm1 {
            body_top: panel::BODY_TOP, body_bot: panel::BODY_BOT, tray: panel::TRAY, well_top: 0x2f2f33, well_bot: 0x27272a,
            silk: panel::SILK, btn_top: panel::BTN_TOP, btn_bot: panel::BTN_BOT, btn_ink: panel::BTN_INK,
            key_top: 0x6a5557, key_bot: 0x4f3f42, key_line: 0x8f7c7e, shade: 0x1a1a1c,
        },
        Theme::Black => Fm1 {
            body_top: 0x2a2b2e, body_bot: 0x1c1d1f, tray: 0x121214, well_top: 0x161719, well_bot: 0x101112,
            silk: 0xe6e6e1, btn_top: 0x2a2b2f, btn_bot: 0x1d1e21, btn_ink: 0xefefea,
            key_top: 0x2c2d31, key_bot: 0x1d1e21, key_line: 0x55575d, shade: 0x0a0a0b,
        },
        Theme::Graphite => Fm1 {
            body_top: 0x434a53, body_bot: 0x343a42, tray: 0x1c1f23, well_top: 0x23272c, well_bot: 0x1b1e22,
            silk: 0xd9dde2, btn_top: 0x262a30, btn_bot: 0x1a1d21, btn_ink: 0xe6eaee,
            key_top: 0x2c3138, key_bot: 0x20242a, key_line: 0x5d6a76, shade: 0x101214,
        },
        Theme::Orange => Fm1 {
            body_top: 0xf3672c, body_bot: 0xe2521e, tray: 0xc8441c, well_top: 0xcf4a21, well_bot: 0xbf411c,
            silk: 0x2a160d, btn_top: 0xe55c2b, btn_bot: 0xcf4c21, btn_ink: 0x3a1a0c,
            key_top: 0xdb552a, key_bot: 0xc5461f, key_line: 0x7a2a12, shade: 0x8a2c10,
        },
        Theme::Mint => Fm1 {
            body_top: 0x2e2f33, body_bot: 0x232427, tray: 0x86cfb7, well_top: 0x8fd6bf, well_bot: 0x7fc8b0,
            silk: 0xcfd2d6, btn_top: 0xa6e6d1, btn_bot: 0x8dd2bb, btn_ink: 0x1d3a32,
            key_top: 0xa9e7d3, key_bot: 0x90d4bd, key_line: 0x2f5c4f, shade: 0x5a9a86,
        },
        Theme::Cream => Fm1 {
            body_top: 0xebe2bd, body_bot: 0xdcd1a8, tray: 0x2f3747, well_top: 0x3a4457, well_bot: 0x2f3748,
            silk: 0x2b2a26, btn_top: 0x3c4555, btn_bot: 0x2d3442, btn_ink: 0xe9e7e0,
            key_top: 0x46516a, key_bot: 0x363f55, key_line: 0x8592ab, shade: 0x1d222c,
        },
    }
}

/// The piano keyboard in the FM-1's groove: (matrix id, x, y, w, h, black), F3..G5.
pub fn fm1_piano_keys() -> Vec<(u8, usize, usize, usize, usize, bool)> {
    // the groove is 34..1246 x 562..798; the whites fill it, 16 of them
    let (x0, y0, ww, wh) = (48.0f32, 576.0f32, 1184.0f32 / 16.0, 208.0f32);
    let whites = [0u8, 2, 4, 6, 7, 9, 11, 12, 14, 16, 18, 19, 21, 23, 24, 26];
    let mut v = Vec::new();
    for (i, n) in whites.iter().enumerate() {
        v.push((14 + n, (x0 + i as f32 * ww + 1.0) as usize, y0 as usize, (ww - 2.0) as usize, wh as usize, false));
    }
    let bw = ww * 0.6;
    let blacks = [(1u8, 0usize), (3, 1), (5, 2), (8, 4), (10, 5), (13, 7), (15, 8), (17, 9), (20, 11), (22, 12), (25, 14)];
    for (n, after) in blacks {
        let cx = x0 + (after + 1) as f32 * ww;
        v.push((14 + n, (cx - bw / 2.0) as usize, y0 as usize, bw as usize, (wh * 0.62) as usize, true));
    }
    v
}

/// The FM-1's note keys as drawn: (id, x, y, w, h, black).
pub fn fm1_keys(k: Keybed) -> Vec<(u8, usize, usize, usize, usize, bool)> {
    match k {
        Keybed::Pills => panel::keys(),
        Keybed::Piano => fm1_piano_keys(),
    }
}

/// What a point of the FM-1 panel is, with its keybed.
pub fn fm1_hit(k: Keybed, x: f32, y: f32) -> Option<panel::Ctl> {
    let h = panel::hit(x, y);
    if k == Keybed::Pills || !matches!(h, Some(panel::Ctl::Key(_)) | None) {
        return h;
    }
    let inside = |px: usize, py: usize, w: usize, hh: usize| x >= px as f32 && x < (px + w) as f32 && y >= py as f32 && y < (py + hh) as f32;
    // black keys first: they lie over the whites
    let mut ks = fm1_piano_keys();
    ks.sort_by_key(|k| !k.5);
    ks.into_iter().find(|&(_, px, py, w, hh, _)| inside(px, py, w, hh)).map(|k| panel::Ctl::Key(k.0))
}

/// A click's velocity at height `y` on key `id`: 1 at its top, 127 at its bottom (panel.rs's).
pub fn fm1_key_vel(k: Keybed, id: u8, y: f32) -> u8 {
    match k {
        Keybed::Pills => panel::key_vel(id, y),
        Keybed::Piano => fm1_piano_keys().iter().find(|q| q.0 == id).map_or(100, |&(_, _, py, _, h, _)| {
            (1.0 + 126.0 * ((y - py as f32) / h as f32).clamp(0.0, 1.0)).round() as u8
        }),
    }
}

/// panel.rs's `base` in theme `t`, the groove as the keybed wants it.
pub fn fm1_base(t: Theme, k: Keybed) -> Vec<u32> {
    if t == Theme::Classic && k == Keybed::Pills {
        return panel::base();
    }
    let c = fm1(t);
    let (w, h) = (panel::W, panel::H);
    canvas(w);
    let mut b = vec![ROOM; w * h];
    rrect(&mut b, 14.0, 14.0, (w - 28) as f32, (h - 28) as f32, 30.0, c.body_top, c.body_bot);
    rrect(&mut b, 352.0, 24.0, 516.0, 516.0, 22.0, 0x0e0e10, 0x0b0b0c);
    rrect(&mut b, 50.0, 388.0, 260.0, 64.0, 14.0, c.tray, c.tray);
    rrect(&mut b, 896.0, 222.0, 360.0, 160.0, 16.0, c.tray, c.tray);
    rrect(&mut b, 34.0, 562.0, 1212.0, 236.0, 26.0, c.well_top, c.well_bot);
    text(&mut b, panel::POT.0, panel::POT.1 - panel::POT.2 - 24.0, "MASTER", 2, c.silk);
    for (name, _, cx, cy, r) in panel::ENCS.iter() {
        text(&mut b, *cx, cy - r - 24.0, name, 2, c.silk);
    }
    rrect(&mut b, 1134.0, 302.0, 112.0, 72.0, 10.0, 0x3c6b43, 0x3c6b43);
    rrect(&mut b, 1136.0, 304.0, 108.0, 68.0, 9.0, c.tray, c.tray);
    if k == Keybed::Piano {
        // the piano's own floor inside the groove
        rrect(&mut b, 44.0, 572.0, 1192.0, 216.0, 8.0, 0x070708, 0x0c0c0d);
    }
    text(&mut b, 640.0, 550.0, "FM-1  FELUCCA", 1, mix(c.silk, c.body_bot, 0.35));
    b
}

/// LED fraction -> brightness (panel.rs's `bright`).
fn bright(f: f32) -> f32 {
    (f / 0.08).clamp(0.0, 1.0)
}

/// panel.rs's `draw` in theme `t`, with keybed `k`.
pub fn fm1_draw(buf: &mut [u32], base: &[u32], st: &panel::State, t: Theme, k: Keybed) {
    if t == Theme::Classic && k == Keybed::Pills {
        return panel::draw(buf, base, st);
    }
    let c = fm1(t);
    let w = panel::W;
    canvas(w);
    buf.copy_from_slice(base);
    for y in 0..240 {
        for x in 0..240 {
            let p = st.lcd[y * 240 + x] as u32;
            let col = ((p >> 11) * 255 / 31) << 16 | (((p >> 5) & 63) * 255 / 63) << 8 | (p & 31) * 255 / 31;
            let o = (panel::LCD_Y + 2 * y) * w + panel::LCD_X + 2 * x;
            buf[o] = col;
            buf[o + 1] = col;
            buf[o + w] = col;
            buf[o + w + 1] = col;
        }
    }
    for (i, (_, _, cx, cy, r)) in panel::ENCS.iter().enumerate() {
        let glow = (st.turning[i] as f32 / 10.0).min(1.0);
        panel::knob(buf, *cx, *cy, *r, (st.enc_clicks[i] as f32 * 18.0).to_radians(), glow);
    }
    let pa = (-135.0 + 270.0 * st.pot as f32 / 1023.0).to_radians();
    panel::knob(buf, panel::POT.0, panel::POT.1, panel::POT.2, pa, (st.turning[7] as f32 / 10.0).min(1.0));
    for (name, id, x, y, bw, bh) in panel::buttons() {
        let down = st.down[id as usize];
        let led = bright(st.leds[id as usize]);
        let tint = if id == 12 { panel::GREEN } else { AMBER };
        let off = if down { 2.0 } else { 0.0 };
        rrect(buf, x as f32 + 1.0, y as f32 + 3.0, bw as f32, bh as f32, 9.0, c.shade, c.shade);
        let (top, bot) = (mix(c.btn_top, tint, led * 0.85), mix(c.btn_bot, tint, led * 0.7));
        rrect(buf, x as f32, y as f32 + off, bw as f32, bh as f32, 9.0, top, bot);
        let ink = if led > 0.5 { 0x2a1a03 } else if id == 13 && t != Theme::Orange { 0xe0483e } else { c.btn_ink };
        text(buf, (x + bw / 2) as f32, (y + bh / 2) as f32 - 7.0 + off, name, 2, ink);
    }
    match k {
        Keybed::Pills => {
            for (id, x, y, kw, kh, _) in panel::keys() {
                let down = st.down[id as usize];
                let led = bright(st.leds[id as usize]);
                let off = if down { 3.0 } else { 0.0 };
                rrect(buf, x as f32 + 1.0, y as f32 + 4.0, kw as f32, kh as f32, kw as f32 / 2.0, c.shade, c.shade);
                let heat = if down { 0.55 } else { led * 0.6 };
                rrect(buf, x as f32, y as f32 + off, kw as f32, kh as f32, kw as f32 / 2.0, mix(c.key_top, AMBER, heat), mix(c.key_bot, AMBER, heat * 0.8));
                let line = mix(c.key_line, 0xffe2b0, if down { 1.0 } else { led });
                rrect(buf, (x + kw / 2) as f32 - 2.5, y as f32 + 18.0 + off, 5.0, kh as f32 - 36.0, 2.5, line, line);
            }
        }
        Keybed::Piano => piano(buf, &fm1_piano_keys(), |id| (st.down[id as usize], bright(st.leds[id as usize]))),
    }
}

/// A piano keyboard: ivory whites with a front lip, glossy blacks over them; held keys sink, a
/// key the firmware lights glows amber (the pills' light, on piano keys).
fn piano(buf: &mut [u32], keys: &[(u8, usize, usize, usize, usize, bool)], state: impl Fn(u8) -> (bool, f32)) {
    let mut ks = keys.to_vec();
    ks.sort_by_key(|k| k.5);
    for (id, x, y, w, h, black) in ks {
        let (x, y, w, h) = (x as f32, y as f32, w as f32, h as f32);
        let (down, led) = state(id);
        if black {
            rrect(buf, x + 3.0, y, w, h + 8.0, 4.0, 0x030303, 0x050506);
            let (top, bot) = if down { (0x101113, 0x1d1e21) } else { (0x16171a, 0x2e3035) };
            let hh = if down { h - 2.0 } else { h - 14.0 };
            rrect(buf, x, y, w, h, 4.0, mix(top, AMBER, led * 0.5), mix(bot, AMBER, led * 0.4));
            rrect(buf, x + 4.0, y, w - 8.0, hh, 3.0, mix(0x0e0f11, AMBER, led * 0.45), mix(if down { 0x15161a } else { 0x1b1c20 }, AMBER, led * 0.4));
            if !down {
                rrect(buf, x + 7.0, y + 6.0, 3.0, hh - 14.0, 1.5, 0x34363c, 0x26282c);
            }
        } else {
            let (top, bot) = if down { (0xd8d4c8, 0xe6e2d6) } else { (0xf1eee6, 0xfbf9f3) };
            let lip = if down { 6.0 } else { 14.0 };
            rrect(buf, x, y, w, h, 5.0, mix(top, AMBER, led * 0.45), mix(bot, AMBER, led * 0.35));
            rrect(buf, x, y + h - lip, w, lip, 5.0, 0xd8d5cc, 0xbdbab2);
            if down {
                rrect(buf, x, y, w, 10.0, 2.0, 0x9c998f, 0xd8d4c8);
            }
        }
    }
}

// ------------------------------------------------------------------ SMK-37

/// The SMK-37's colours (panel_smk.rs's constants, as a theme).
struct Smk {
    edge: u32,
    edge_in: u32,
    body_top: u32,
    body_bot: u32,
    ink: u32,
    ink_dim: u32,
    logo: u32,
    cap_top: u32,
    cap_bot: u32,
    cap_ink: u32,
    pad_top: u32,
    pad_bot: u32,
}

fn smk(t: Theme) -> Smk {
    match t {
        // (Classic draws with panel_smk.rs itself; these are its constants, for the thumbnails)
        Theme::Classic | Theme::Black => Smk {
            edge: 0x3a3c41, edge_in: 0x2a2b2f, body_top: 0x222327, body_bot: 0x18191c, ink: 0xb9bbbf, ink_dim: 0x7d8085,
            logo: 0xe4e5e8, cap_top: 0x2b2c31, cap_bot: 0x1f2024, cap_ink: 0x9fa2a8, pad_top: 0x2c2d32, pad_bot: 0x1d1e22,
        },
        Theme::Graphite => Smk {
            edge: 0x5a626c, edge_in: 0x48505a, body_top: 0x434a53, body_bot: 0x343a42, ink: 0xd9dde2, ink_dim: 0x9aa3ad,
            logo: 0xeef1f4, cap_top: 0x262a30, cap_bot: 0x1a1d21, cap_ink: 0xc5cbd2, pad_top: 0x2a2f35, pad_bot: 0x1e2227,
        },
        Theme::Orange => Smk {
            edge: 0xff8a52, edge_in: 0xf06a30, body_top: 0xf3672c, body_bot: 0xe2521e, ink: 0x2a160d, ink_dim: 0x6e2c12,
            logo: 0x1e0f08, cap_top: 0xe55c2b, cap_bot: 0xcf4c21, cap_ink: 0x3a1a0c, pad_top: 0xd9532a, pad_bot: 0xbf451f,
        },
        Theme::Mint => Smk {
            edge: 0x45474c, edge_in: 0x34363a, body_top: 0x2e2f33, body_bot: 0x232427, ink: 0xcfd2d6, ink_dim: 0x8fd6bf,
            logo: 0x9fe2cc, cap_top: 0xa6e6d1, cap_bot: 0x8dd2bb, cap_ink: 0x1d3a32, pad_top: 0x9fe0cb, pad_bot: 0x85cdb5,
        },
        Theme::Cream => Smk {
            edge: 0xf6eecd, edge_in: 0xe4dab2, body_top: 0xebe2bd, body_bot: 0xdcd1a8, ink: 0x2b2a26, ink_dim: 0x6f6a58,
            logo: 0x2f3747, cap_top: 0x3c4555, cap_bot: 0x2d3442, cap_ink: 0xe9e7e0, pad_top: 0x46516a, pad_bot: 0x363f55,
        },
    }
}

/// panel_smk.rs's `base` in theme `t`.
pub fn smk_base(t: Theme) -> Vec<u32> {
    if t == Theme::Classic {
        return panel_smk::base();
    }
    let c = smk(t);
    let (w, h) = (panel_smk::W, panel_smk::H);
    let (lx, ly) = (panel_smk::LCD_X, panel_smk::LCD_Y);
    canvas(w);
    let mut b = vec![ROOM; w * h];
    rrect(&mut b, 8.0, 8.0, (w - 16) as f32, (h - 16) as f32, 30.0, c.edge, c.edge_in);
    rrect(&mut b, 11.0, 11.0, (w - 22) as f32, (h - 22) as f32, 28.0, c.body_top, c.body_bot);
    rrect(&mut b, (lx - 16) as f32, (ly - 16) as f32, 512.0, 512.0, 16.0, 0x34363b, 0x2a2b2f);
    rrect(&mut b, (lx - 14) as f32, (ly - 14) as f32, 508.0, 508.0, 15.0, 0x060607, 0x09090a);
    for (x, y, ww, hh) in panel_smk::WHEELS {
        rrect(&mut b, x as f32 - 8.0, y as f32 - 10.0, ww as f32 + 16.0, hh as f32 + 20.0, 12.0, 0x08080a, 0x0e0e10);
    }
    text(&mut b, panel_smk::WHEELS[0].0 as f32 + 34.0, 40.0, "PITCH", 2, c.ink);
    text(&mut b, panel_smk::WHEELS[1].0 as f32 + 34.0, 40.0, "MOD", 2, c.ink);
    text(&mut b, 138.0, 312.0, "M-VAVE", 4, c.logo);
    text(&mut b, 138.0, 364.0, "PAD BANK", 1, c.ink);
    rrect(&mut b, 52.0, 367.0, 46.0, 1.0, 0.5, c.ink_dim, c.ink_dim);
    rrect(&mut b, 178.0, 367.0, 46.0, 1.0, 0.5, c.ink_dim, c.ink_dim);
    for (i, (cx, cy, r)) in panel_smk::encs().iter().enumerate() {
        text(&mut b, *cx, cy - r - 22.0, &format!("K{}", i + 1), 1, c.ink);
    }
    for (k, (x, y, pw, ph)) in panel_smk::pads().iter().enumerate().skip(8) {
        smk_symbol(&mut b, k - 8, (x + pw / 2) as f32, (y + ph + 18) as f32, c.ink_dim, c.body_top);
    }
    for (i, (fx, fy, len)) in panel_smk::faders().iter().enumerate() {
        text(&mut b, *fx as f32, *fy as f32 - 48.0, &format!("FADER{}", i + 1), 1, c.ink);
        rrect(&mut b, *fx as f32 - 4.0, *fy as f32 - 6.0, 8.0, *len as f32 + 12.0, 4.0, 0x050506, 0x0a0a0b);
        for tk in 0..=10 {
            let y = *fy as f32 + tk as f32 * *len as f32 / 10.0;
            let tw = if tk % 5 == 0 { 10.0 } else { 6.0 };
            rrect(&mut b, *fx as f32 - 34.0, y - 0.5, tw, 1.0, 0.5, c.ink_dim, c.ink_dim);
        }
    }
    rrect(&mut b, 38.0, 572.0, (w - 76) as f32, 294.0, 8.0, 0x060607, 0x0b0b0c);
    for (id, x, _, kw, _, black) in panel_smk::keys() {
        let s = smk_key_print(id as usize);
        if !s.is_empty() {
            text(&mut b, (x + kw / 2) as f32, if black { 546.0 } else { 557.0 }, s, 1, c.ink_dim);
        }
    }
    b
}

/// panel_smk.rs's `draw` in theme `t`.
pub fn smk_draw(buf: &mut [u32], base: &[u32], st: &panel_smk::State, t: Theme) {
    if t == Theme::Classic {
        return panel_smk::draw(buf, base, st);
    }
    const LED: u32 = 0xe9eefc;
    const LED_BT: u32 = 0x5b8cff;
    let c = smk(t);
    let w = panel_smk::W;
    let btn0: u8 = 37;
    canvas(w);
    buf.copy_from_slice(base);
    for y in 0..240 {
        for x in 0..240 {
            let p = st.lcd[y * 240 + x] as u32;
            let col = ((p >> 11) * 255 / 31) << 16 | (((p >> 5) & 63) * 255 / 63) << 8 | (p & 31) * 255 / 31;
            let o = (panel_smk::LCD_Y + 2 * y) * w + panel_smk::LCD_X + 2 * x;
            buf[o] = col;
            buf[o + 1] = col;
            buf[o + w] = col;
            buf[o + w + 1] = col;
        }
    }
    for (i, (cx, cy, r)) in panel_smk::encs().iter().enumerate() {
        let glow = (st.turning[i] as f32 / 10.0).min(1.0);
        smk_knob(buf, *cx, *cy, *r, (st.enc_clicks[i] as f32 * 18.0).to_radians(), glow, c.body_top);
    }
    for (p, (x, y, pw, ph)) in panel_smk::pads().iter().enumerate() {
        let (x, y, pw, ph) = (*x as f32, *y as f32, *pw as f32, *ph as f32);
        let col = st.pads[p];
        let force = st.pad_force[p] as f32 / 1023.0;
        let lit = |s: u32| ((((col >> s) & 255) as f32 / 255.0).powf(0.6) * 255.0) as u32;
        let rgb = lit(16) << 16 | lit(8) << 8 | lit(0);
        let on = col != 0;
        let off = if force > 0.0 { 2.0 } else { 0.0 };
        if on {
            for py in (y - 8.0) as i32..(y + ph + 14.0) as i32 {
                for px in (x - 10.0) as i32..(x + pw + 10.0) as i32 {
                    let (fx, fy) = (px as f32 + 0.5, py as f32 + 0.5);
                    let dx = (x - fx).max(fx - (x + pw)).max(0.0);
                    let dy = (y - fy).max(fy - (y + ph)).max(0.0);
                    let d = (dx * dx + dy * dy).sqrt();
                    let below = ((fy - y) / ph).clamp(0.0, 1.0);
                    let a = (1.0 - d / 11.0).clamp(0.0, 1.0).powi(2) * (0.25 + 0.6 * below);
                    if d > 0.0 {
                        put(buf, px, py, rgb, a);
                    }
                }
            }
        } else {
            rrect(buf, x + 1.0, y + 3.0, pw, ph, 7.0, 0x0c0c0e, 0x0c0c0e);
        }
        let bot = if on { mix(c.pad_bot, rgb, 0.22 + 0.25 * force) } else { c.pad_bot };
        rrect(buf, x, y + off, pw, ph, 7.0, mix(c.pad_top, rgb, if on { 0.05 + 0.15 * force } else { 0.0 }), bot);
        if on {
            rrect(buf, x + 3.0, y + ph - 7.0 + off, pw - 6.0, 5.0, 2.5, mix(rgb, 0xffffff, 0.15), rgb);
        }
    }
    for (i, (fx, fy, len)) in panel_smk::faders().iter().enumerate() {
        let tt = st.faders[i] as f32 / 1023.0;
        let (fx, cy) = (*fx as f32, *fy as f32 + (1.0 - tt) * *len as f32);
        rrect(buf, fx - 21.0, cy - 17.0 + 4.0, 42.0, 34.0, 5.0, 0x060607, 0x060607);
        rrect(buf, fx - 21.0, cy - 17.0, 42.0, 34.0, 5.0, 0x3a3c42, 0x16171a);
        rrect(buf, fx - 19.0, cy - 15.0, 38.0, 8.0, 3.0, 0x4a4d54, 0x33353a);
        rrect(buf, fx - 17.0, cy - 1.0, 34.0, 2.0, 1.0, 0xe6e6e2, 0xe6e6e2);
    }
    for (i, (x, y, ww, hh)) in panel_smk::WHEELS.iter().enumerate() {
        let tt = if i == 0 { (640.0 - st.pitch as f32) / (640.0 - 445.0) } else { (st.modw as f32 - 474.0) / (629.0 - 474.0) };
        smk_wheel(buf, *x as f32, *y as f32, *ww as f32, *hh as f32, tt, c.ink);
    }
    for (name, id, x, y, bw, bh) in panel_smk::buttons() {
        let (x, y, bw, bh) = (x as f32, y as f32, bw as f32, bh as f32);
        let down = st.down[id as usize];
        let led = ((st.leds[(id - btn0) as usize] - 0.15) / 0.35).clamp(0.0, 1.0);
        let lc = if name == "BT" { LED_BT } else { LED };
        let off = if down { 2.0 } else { 0.0 };
        rrect(buf, x + 1.0, y + 3.0, bw, bh, 7.0, 0x09090a, 0x09090a);
        if led > 0.0 {
            rrect(buf, x - 2.0, y - 2.0 + off, bw + 4.0, bh + 4.0, 9.0, mix(c.body_top, lc, led * 0.45), mix(c.body_bot, lc, led * 0.35));
        }
        let (top, bot) = if down { (mix(c.cap_top, 0x000000, 0.18), mix(c.cap_bot, 0x000000, 0.12)) } else { (c.cap_top, c.cap_bot) };
        rrect(buf, x, y + off, bw, bh, 7.0, mix(top, lc, led * 0.12), mix(bot, lc, led * 0.08));
        smk_label(buf, x + bw / 2.0, y + bh / 2.0 + off, bw - 8.0, name, mix(c.cap_ink, lc, led));
    }
    piano(buf, &panel_smk::keys(), |id| (st.down[id as usize], 0.0));
}

// (panel_smk.rs's private helpers, as they are there, their colours passed in)

fn smk_knob(buf: &mut [u32], cx: f32, cy: f32, r: f32, angle: f32, glow: f32, body: u32) {
    disc(buf, cx + 1.5, cy + 4.0, r + 2.0, |_, _| 0x0c0c0e);
    if glow > 0.0 {
        disc(buf, cx, cy, r + 4.0, |_, _| mix(body, AMBER, glow * 0.7));
    }
    disc(buf, cx, cy, r, |dx, dy| {
        let a = dy.atan2(dx);
        let flute = ((a * 22.0).sin() * 0.5 + 0.5).powf(0.6);
        mix(0x2a2c30, 0x5e6168, flute * 0.55 + (-dy / r) * 0.25 + 0.15)
    });
    disc(buf, cx, cy, r * 0.74, |dx, dy| {
        let l = (-(dx * 0.4 + dy) / (r * 0.9)).clamp(-1.0, 1.0) * 0.5 + 0.5;
        mix(0x45484e, 0x6e727a, l)
    });
    let (s, c) = angle.sin_cos();
    for t in 0..((r * 0.42) as i32) {
        let d = r * 0.28 + t as f32;
        disc(buf, cx + s * d, cy - c * d, 1.4, |_, _| 0xf2f2ee);
    }
}

fn smk_wheel(buf: &mut [u32], x: f32, y: f32, w: f32, h: f32, t: f32, ink: u32) {
    let (cy, rr) = (y + h / 2.0, h / 2.0);
    let phase = t * 5.0;
    for py in y as i32..(y + h) as i32 {
        let v = ((py as f32 + 0.5 - cy) / rr).clamp(-0.999, 0.999);
        let th = v.asin();
        let shade = (th.cos() * 0.85 - v * 0.25).clamp(0.0, 1.0);
        let rib = ((th * 18.0 + phase).sin() * 0.5 + 0.5).powf(6.0);
        for px in x as i32..(x + w) as i32 {
            let e = ((px as f32 + 0.5 - x).min(x + w - px as f32 - 0.5) / 5.0).clamp(0.0, 1.0);
            let col = mix(0x0b0b0d, mix(0x34363b, 0x5c5f66, rib), shade * (0.45 + 0.55 * e));
            put(buf, px, py, col, 1.0);
        }
    }
    let my = y + t.clamp(0.0, 1.0) * h;
    rrect(buf, x - 7.0, my - 1.5, 5.0, 3.0, 1.0, ink, ink);
}

fn smk_tri(buf: &mut [u32], p: [(f32, f32); 3], c: u32) {
    let (x0, x1) = (p.iter().map(|q| q.0).fold(f32::MAX, f32::min), p.iter().map(|q| q.0).fold(f32::MIN, f32::max));
    let (y0, y1) = (p.iter().map(|q| q.1).fold(f32::MAX, f32::min), p.iter().map(|q| q.1).fold(f32::MIN, f32::max));
    let edge = |a: (f32, f32), b: (f32, f32), x: f32, y: f32| (b.0 - a.0) * (y - a.1) - (b.1 - a.1) * (x - a.0);
    let area = edge(p[0], p[1], p[2].0, p[2].1);
    for py in y0.floor() as i32..=y1.ceil() as i32 {
        for px in x0.floor() as i32..=x1.ceil() as i32 {
            let mut n = 0;
            for s in 0..16 {
                let (x, y) = (px as f32 + (s % 4) as f32 / 4.0 + 0.125, py as f32 + (s / 4) as f32 / 4.0 + 0.125);
                let (a, b, cc) = (edge(p[1], p[2], x, y), edge(p[2], p[0], x, y), edge(p[0], p[1], x, y));
                if (a * area >= 0.0) && (b * area >= 0.0) && (cc * area >= 0.0) {
                    n += 1;
                }
            }
            put(buf, px, py, c, n as f32 / 16.0);
        }
    }
}

fn smk_label(buf: &mut [u32], cx: f32, cy: f32, w: f32, s: &str, color: u32) {
    match s.split_once(' ').filter(|_| (s.len() * 6) as f32 > w) {
        Some((a, b)) => {
            text(buf, cx, cy - 9.0, a, 1, color);
            text(buf, cx, cy + 2.0, b, 1, color);
        }
        None => text(buf, cx, cy - 3.0, s, 1, color),
    }
}

fn smk_symbol(buf: &mut [u32], k: usize, cx: f32, cy: f32, c: u32, body: u32) {
    match k {
        0 => smk_tri(buf, [(cx - 5.0, cy - 6.0), (cx - 5.0, cy + 6.0), (cx + 6.0, cy)], c),
        1 => {
            rrect(buf, cx - 5.0, cy - 6.0, 3.0, 12.0, 0.5, c, c);
            rrect(buf, cx + 2.0, cy - 6.0, 3.0, 12.0, 0.5, c, c);
        }
        2 => disc(buf, cx, cy, 5.5, |_, _| c),
        3 | 4 => {
            let s = if k == 3 { -1.0 } else { 1.0 };
            for o in [-5.0, 2.0] {
                let x = cx + o * s;
                smk_tri(buf, [(x - 3.0 * s, cy - 6.0), (x - 3.0 * s, cy + 6.0), (x + 4.0 * s, cy)], c);
            }
        }
        5 | 6 => {
            let s = if k == 5 { -1.0 } else { 1.0 };
            smk_tri(buf, [(cx - 6.0 * s, cy - 7.0), (cx - 6.0 * s, cy + 7.0), (cx + 6.0 * s, cy)], c);
            smk_tri(buf, [(cx - 3.5 * s, cy - 3.5), (cx - 3.5 * s, cy + 3.5), (cx + 2.0 * s, cy)], body);
        }
        _ => text(buf, cx, cy - 3.0, "UNDO", 1, c),
    }
}

fn smk_key_print(k: usize) -> &'static str {
    const P: [&str; 37] = [
        "CHN 1", "CHN 2", "CHN 3", "CHN 4", "CHN 5", "CHN 6", "CHN 7", "CHN 8", "CHN 9", "CHN 10", "CHN 11", "CHN 12",
        "CHN 13", "CHN 14", "CHN 15", "CHN 16", "PRE 1", "PRE 2", "PRE 3", "PRE 4", "PRE 5", "PRE 6", "PRE 7", "PRE 8",
        "1/4", "1/4T", "1/8", "1/8T", "1/16", "1/16T", "1/32", "1/32T", "", "", "", "", "RESET",
    ];
    P[k]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The piano in the groove: 27 keys F3..G5 with the pills' ids, each hit where it is drawn,
    /// the blacks over the whites; the other controls as before.
    #[test]
    fn piano_keys_hit_where_drawn() {
        let ks = fm1_piano_keys();
        assert_eq!(ks.len(), 27);
        let mut ids: Vec<u8> = ks.iter().map(|k| k.0).collect();
        ids.sort();
        assert_eq!(ids, (14..=40).collect::<Vec<u8>>());
        for &(id, x, y, w, h, black) in &ks {
            // a black key at its middle; a white key low down, below the blacks
            let (px, py) = (x as f32 + w as f32 / 2.0, y as f32 + if black { h as f32 / 2.0 } else { h as f32 - 10.0 });
            assert_eq!(fm1_hit(Keybed::Piano, px, py), Some(panel::Ctl::Key(id)), "{id}");
        }
        assert_eq!(fm1_hit(Keybed::Piano, 250.0, 135.0), Some(panel::Ctl::Enc(0)));
        assert!(fm1_key_vel(Keybed::Piano, 21, 576.0) < 5 && fm1_key_vel(Keybed::Piano, 21, 783.0) > 120);
        // every theme draws
        for t in Theme::ALL {
            for k in [Keybed::Pills, Keybed::Piano] {
                assert_eq!(fm1_base(t, k).len(), panel::W * panel::H);
            }
            assert_eq!(smk_base(t).len(), panel_smk::W * panel_smk::H);
        }
    }
}
