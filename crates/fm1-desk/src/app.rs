//! The window: the playing synth's panel (fm1-live's drawings and mouse / keyboard handling),
//! a slim status bar, and the firmware picker over the panel, full window.

use crate::catalog::{BoardId, Catalog, Entry, Local, Source, Status, Store};
use crate::engine::{self, Audio, Midi, Session};
use crate::keys::{self, Layout};
use crate::skin::{self, Keybed, Theme};
use crate::{panel, panel_smk};
use eframe::egui::{self, Color32, Key, RichText};
use fm1_soc::Action;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

pub const TITLE: &str = "FM-1 · SMK-37 Desk";
/// Pixels of vertical mouse drag per encoder click (fm1-live's).
const DRAG_PER_CLICK: f32 = 12.0;
/// Scroll distance per wheel notch, in points (egui's line height for a wheel step).
const NOTCH: f32 = 50.0;
const CLOCKS: [u64; 5] = [240, 200, 160, 120, 96];
/// The Library strip's width.
const LIBRARY_W: f32 = 236.0;

// ------------------------------------------------------------------ settings

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
struct Settings {
    board: BoardId,
    /// the firmware last played per board
    last: BTreeMap<String, String>,
    /// M-VAVE's own packages the user opened (a path per board)
    own: BTreeMap<String, String>,
    mhz: u64,
    midi: String,
    /// the computer keyboard's layout, and its letters drawn on the panel's keys
    keys: Layout,
    hints: bool,
    /// the Library strip on the left (F2)
    library: bool,
    /// firmwares played, the last first ("<board>/<id>")
    recent: Vec<String>,
    /// each board's colours, and the FM-1's keybed (pills or a piano keyboard)
    theme_fm1: Theme,
    theme_smk: Theme,
    keybed: Keybed,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { board: BoardId::Fm1, last: BTreeMap::new(), own: BTreeMap::new(), mhz: 240, midi: "all".into(), keys: Layout::Ableton, hints: false, library: false, recent: Vec::new(), theme_fm1: Theme::Classic, theme_smk: Theme::Classic, keybed: Keybed::Pills }
    }
}

impl Settings {
    fn path(store: &Store) -> PathBuf {
        store.root.join("fm1-desk.json")
    }
    fn load(store: &Store) -> Settings {
        std::fs::read_to_string(Self::path(store)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }
    fn save(&self, store: &Store) {
        if let Ok(t) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(Self::path(store), t + "\n");
        }
    }
}

// ------------------------------------------------------------------ the panels' input state

/// The FM-1's controls as the window holds them (fm1-live's `main` loop state).
struct Fm1Input {
    down: [bool; 41],
    enc_clicks: [i32; 7],
    turning: [u32; 8],
    scroll_acc: [f32; 8],
    shown_leds: [f32; 41],
    grab: Option<panel::Ctl>,
    last_y: f32,
    drag_acc: f32,
    pot: u16,
    /// the key under the mouse: how hard (its height; lower = harder)
    mouse_vel: u8,
}

/// The SMK-37's (fm1-live's `window_smk` state).
struct SmkInput {
    down: [bool; 64],
    enc_clicks: [i32; 8],
    turning: [u32; 8],
    scroll_acc: [f32; 8],
    analog: [u16; 23],
    grab: Option<panel_smk::Ctl>,
    last_y: f32,
    drag_acc: f32,
}

impl Fm1Input {
    fn new(pot: u16) -> Self {
        Fm1Input { down: [false; 41], enc_clicks: [0; 7], turning: [0; 8], scroll_acc: [0.0; 8], shown_leds: [0.0; 41], grab: None, last_y: 0.0, drag_acc: 0.0, pot, mouse_vel: 0 }
    }
}

impl SmkInput {
    fn new() -> Self {
        use fm1_soc::smk::{ANALOG_MOD, ANALOG_PITCH, MOD_REST, PITCH_REST};
        let mut analog = [0u16; 23];
        analog[ANALOG_PITCH as usize] = PITCH_REST;
        analog[ANALOG_MOD as usize] = MOD_REST;
        SmkInput { down: [false; 64], enc_clicks: [0; 8], turning: [0; 8], scroll_acc: [0.0; 8], analog, grab: None, last_y: 0.0, drag_acc: 0.0 }
    }
}

enum Input {
    Fm1(Fm1Input),
    Smk(SmkInput),
}

/// What the window reads of the computer's keyboard and mouse this frame, in panel pixels.
struct Frame {
    keys: HashSet<Key>,
    /// keys pressed this frame, auto-repeat included (the encoders' arrows)
    pressed: Vec<Key>,
    pointer: Option<(f32, f32)>,
    down: bool,
    was_down: bool,
    scroll: f32,
}

// ------------------------------------------------------------------ the app

struct Running {
    session: Session,
    board: BoardId,
    /// catalog id, or "own:<file>" for a package the user opened
    id: String,
    name: String,
    version: String,
    started: Instant,
}

struct Lists {
    cat: Catalog,
    source: Source,
    locals: BTreeMap<String, Local>,
    fetching: bool,
    fetch_err: Option<String>,
}

enum Job {
    Catalog(BoardId, Result<Catalog, String>),
    Downloaded { board: BoardId, id: String, result: Result<PathBuf, String>, play: bool },
}

pub struct Desk {
    store: Store,
    settings: Settings,
    audio: Audio,
    midi: Midi,
    midi_ports: Vec<String>,
    running: Option<Running>,
    input: Input,
    lists: HashMap<BoardId, Lists>,
    jobs_tx: mpsc::Sender<Job>,
    jobs_rx: mpsc::Receiver<Job>,
    busy: HashSet<(BoardId, String)>,
    errors: HashMap<(BoardId, String), String>,
    // the picker
    picker: bool,
    tab: BoardId,
    search: String,
    focus_search: bool,
    // the panel picture
    buf: Vec<u32>,
    fb: Vec<u16>,
    tex: Option<egui::TextureHandle>,
    was_down: bool,
    message: Option<(String, Instant)>,
    title: String,
    ctx: egui::Context,
    // the Library strip
    board_tex: HashMap<BoardId, egui::TextureHandle>,
    thumbs: HashMap<String, Option<egui::TextureHandle>>,
    live_tex: Option<egui::TextureHandle>,
    live_at: Instant,
}

impl Desk {
    pub fn new(cc: &eframe::CreationContext) -> Desk {
        let mut style = (*cc.egui_ctx.style()).clone();
        style.text_styles.insert(egui::TextStyle::Body, egui::FontId::proportional(15.0));
        style.text_styles.insert(egui::TextStyle::Button, egui::FontId::proportional(15.0));
        cc.egui_ctx.set_style(style);
        cc.egui_ctx.set_visuals(egui::Visuals::dark());

        let store = Store::here();
        let settings = Settings::load(&store);
        let audio = Audio::open();
        let mut midi = Midi::new();
        midi.connect(&settings.midi);
        let (jobs_tx, jobs_rx) = mpsc::channel();
        let mut lists = HashMap::new();
        for b in BoardId::ALL {
            let (cat, source) = store.catalog(b);
            lists.insert(b, Lists { cat, source, locals: store.locals(b), fetching: true, fetch_err: None });
            // upstream's lists, fresh, in the background
            let (tx, root, ctx) = (jobs_tx.clone(), store.root.clone(), cc.egui_ctx.clone());
            std::thread::spawn(move || {
                let _ = tx.send(Job::Catalog(b, Store { root }.fetch_catalog(b)));
                ctx.request_repaint();
            });
        }
        let mut desk = Desk {
            tab: settings.board,
            store,
            settings,
            audio,
            midi,
            midi_ports: Midi::list(),
            running: None,
            input: Input::Fm1(Fm1Input::new(512)),
            lists,
            jobs_tx,
            jobs_rx,
            busy: HashSet::new(),
            errors: HashMap::new(),
            picker: true,
            search: String::new(),
            focus_search: true,
            buf: Vec::new(),
            fb: vec![0u16; 240 * 240],
            tex: None,
            was_down: false,
            message: None,
            title: String::new(),
            ctx: cc.egui_ctx.clone(),
            board_tex: HashMap::new(),
            thumbs: HashMap::new(),
            live_tex: None,
            live_at: Instant::now(),
        };
        if let Some(e) = desk.audio.error.clone() {
            desk.say(format!("no sound: {e}"));
        }
        // the firmware last played comes back; else the picker
        let b = desk.settings.board;
        if let Some(id) = desk.settings.last.get(b.dir()).cloned() {
            desk.play_id(b, &id);
        }
        desk
    }

    fn say(&mut self, m: String) {
        self.message = Some((m, Instant::now()));
    }

    // -------------------------------------------------------------- playing

    /// Play catalog firmware `id` of board `b`: its package if it is here (downloaded first if not).
    fn play_id(&mut self, b: BoardId, id: &str) {
        if let Some(own) = id.strip_prefix("own:") {
            if let Some(path) = self.settings.own.get(b.dir()).cloned().filter(|p| p.ends_with(own)) {
                self.play_file(PathBuf::from(path));
            }
            return;
        }
        let Some(entry) = self.lists[&b].cat.firmwares.iter().find(|e| e.id == id).cloned() else { return };
        match self.store.package_path(b, id) {
            Some(p) => self.play_path(b, &entry, p),
            None => self.download(b, &entry, true),
        }
    }

    fn play_path(&mut self, b: BoardId, e: &Entry, path: PathBuf) {
        let version = self.lists[&b].locals.get(&e.id).map(|l| l.version.clone()).unwrap_or_else(|| e.version.clone());
        self.start(path, b, e.id.clone(), e.name.clone(), version);
    }

    /// A package the user opened (M-VAVE's own, or any .fwsc).
    fn play_file(&mut self, path: PathBuf) {
        let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let b = match std::fs::read(&path).map_err(|e| e.to_string()).and_then(|p| engine::board_of(&p)) {
            Ok(b) => BoardId::of(b),
            Err(e) => return self.say(format!("{}: {e}", path.display())),
        };
        let file = path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        self.settings.own.insert(b.dir().into(), path.to_string_lossy().into_owned());
        self.start(path, b, format!("own:{file}"), stem, String::new());
    }

    fn start(&mut self, path: PathBuf, b: BoardId, id: String, name: String, version: String) {
        let pkg = match std::fs::read(&path) {
            Ok(p) => p,
            Err(e) => return self.say(format!("{}: {e}", path.display())),
        };
        if let Some(r) = self.running.take() {
            self.snapshot(&r);
            r.session.stop(&self.midi);
        }
        let save = self.store.save_path(b, &id.replace("own:", "own-"));
        let before = self.running.as_ref().map(|r| r.board);
        match Session::start(&pkg, save, self.settings.mhz, &self.audio, &self.midi) {
            Ok(s) => {
                self.input = match s.board {
                    fm1_soc::Board::Fm1 => Input::Fm1(Fm1Input::new(s.pot0)),
                    fm1_soc::Board::Smk37 => Input::Smk(SmkInput::new()),
                };
                let board = BoardId::of(s.board);
                self.running = Some(Running { session: s, board, id: id.clone(), name, version, started: Instant::now() });
                self.settings.board = board;
                let key = format!("{}/{id}", board.dir());
                self.settings.recent.retain(|k| *k != key);
                self.settings.recent.insert(0, key);
                self.settings.recent.truncate(60);
                self.settings.last.insert(board.dir().into(), id);
                self.settings.save(&self.store);
                self.picker = false;
                if before != Some(board) {
                    self.fit_window();
                }
            }
            Err(e) => self.say(format!("{name}: {e}")),
        }
    }

    fn download(&mut self, b: BoardId, e: &Entry, play: bool) {
        let key = (b, e.id.clone());
        if !self.busy.insert(key.clone()) {
            return;
        }
        self.errors.remove(&key);
        let (tx, root, e) = (self.jobs_tx.clone(), self.store.root.clone(), e.clone());
        std::thread::spawn(move || {
            let result = Store { root }.download(b, &e);
            let _ = tx.send(Job::Downloaded { board: b, id: e.id.clone(), result, play });
        });
    }

    fn poll_jobs(&mut self) {
        while let Ok(j) = self.jobs_rx.try_recv() {
            match j {
                Job::Catalog(b, r) => {
                    let l = self.lists.get_mut(&b).unwrap();
                    l.fetching = false;
                    match r {
                        Ok(c) => {
                            l.cat = c;
                            l.source = Source::Fetched;
                            l.fetch_err = None;
                        }
                        Err(e) => l.fetch_err = Some(e),
                    }
                }
                Job::Downloaded { board, id, result, play } => {
                    self.busy.remove(&(board, id.clone()));
                    self.lists.get_mut(&board).unwrap().locals = self.store.locals(board);
                    match result {
                        Ok(path) => {
                            if play {
                                if let Some(e) = self.lists[&board].cat.firmwares.iter().find(|e| e.id == id).cloned() {
                                    self.play_path(board, &e, path);
                                }
                            }
                        }
                        Err(e) => {
                            self.errors.insert((board, id), e);
                        }
                    }
                }
            }
        }
    }

    fn refetch(&mut self, ctx: &egui::Context) {
        for b in BoardId::ALL {
            let l = self.lists.get_mut(&b).unwrap();
            if l.fetching {
                continue;
            }
            l.fetching = true;
            let (tx, root, ctx) = (self.jobs_tx.clone(), self.store.root.clone(), ctx.clone());
            std::thread::spawn(move || {
                let _ = tx.send(Job::Catalog(b, Store { root }.fetch_catalog(b)));
                ctx.request_repaint();
            });
        }
    }

    fn updates(&self, b: BoardId) -> Vec<Entry> {
        let l = &self.lists[&b];
        l.cat.firmwares.iter().filter(|e| matches!(self.store.status(b, e, &l.locals), Status::Update(_))).cloned().collect()
    }

    // -------------------------------------------------------------- the panel

    fn panel_size(&self) -> (usize, usize) {
        match self.input {
            Input::Fm1(_) => (panel::W, panel::H),
            Input::Smk(_) => (panel_smk::W, panel_smk::H),
        }
    }

    /// The panel at its size inside `avail`, centred.
    fn panel_rect(&self, avail: egui::Rect) -> egui::Rect {
        let (w, h) = self.panel_size();
        let s = (avail.width() / w as f32).min(avail.height() / h as f32);
        egui::Rect::from_center_size(avail.center(), egui::vec2(w as f32 * s, h as f32 * s))
    }

    /// This frame's keyboard and mouse, the mouse in panel pixels.
    fn read_input(&mut self, ctx: &egui::Context, rect: egui::Rect, own_keys: bool) -> Frame {
        let (w, h) = self.panel_size();
        let f = ctx.input(|i| {
            let pointer = i.pointer.latest_pos().map(|p| {
                let x = ((p.x - rect.min.x) / rect.width() * w as f32).clamp(0.0, w as f32 - 1.0);
                let y = ((p.y - rect.min.y) / rect.height() * h as f32).clamp(0.0, h as f32 - 1.0);
                (x, y)
            });
            let inside = i.pointer.latest_pos().is_some_and(|p| rect.contains(p));
            let mut pressed = Vec::new();
            for e in &i.events {
                if let egui::Event::Key { key, pressed: true, .. } = e {
                    pressed.push(*key);
                }
            }
            Frame {
                keys: if own_keys { i.keys_down.clone() } else { HashSet::new() },
                pressed: if own_keys { pressed } else { Vec::new() },
                pointer,
                down: i.pointer.primary_down() && (inside || self.was_down),
                was_down: self.was_down,
                scroll: if inside { i.raw_scroll_delta.y / NOTCH } else { 0.0 },
            }
        });
        self.was_down = f.down;
        f
    }

    /// fm1-live's FM-1 window loop, one frame.
    fn fm1_frame(&mut self, f: &Frame) {
        let layout = self.settings.keys;
        let kb = self.settings.keybed;
        let (Some(r), Input::Fm1(s)) = (&self.running, &mut self.input) else { return };
        let send = |a: Action| r.session.send(a);
        let turn = |s: &mut Fm1Input, i: usize, clicks: i32| {
            s.enc_clicks[i] += clicks;
            s.turning[i] = 10;
            send(Action::Enc { enc: panel::ENCS[i].1, clicks });
        };
        if let Some((mx, my)) = f.pointer {
            if f.down && !f.was_down {
                s.grab = skin::fm1_hit(kb, mx, my);
                s.last_y = my;
                s.drag_acc = 0.0;
                if let Some(panel::Ctl::Key(k)) = s.grab {
                    s.mouse_vel = skin::fm1_key_vel(kb, k, my);
                }
            } else if f.down {
                match s.grab {
                    // glissando: the held note follows the pointer across keys; on the same key,
                    // its height is the pressure (aftertouch)
                    Some(panel::Ctl::Key(k0)) => {
                        if let Some(panel::Ctl::Key(k)) = skin::fm1_hit(kb, mx, my) {
                            let v = skin::fm1_key_vel(kb, k, my);
                            if k != k0 {
                                s.grab = Some(panel::Ctl::Key(k));
                                s.mouse_vel = v;
                            } else if v.abs_diff(s.mouse_vel) >= 4 {
                                s.mouse_vel = v;
                                send(Action::Pressure { id: k, vel: v });
                            }
                        }
                    }
                    Some(panel::Ctl::Enc(i)) => {
                        s.drag_acc += s.last_y - my;
                        while s.drag_acc >= DRAG_PER_CLICK {
                            s.drag_acc -= DRAG_PER_CLICK;
                            turn(s, i, 1);
                        }
                        while s.drag_acc <= -DRAG_PER_CLICK {
                            s.drag_acc += DRAG_PER_CLICK;
                            turn(s, i, -1);
                        }
                    }
                    Some(panel::Ctl::Pot) => {
                        let v = (s.pot as f32 + (s.last_y - my) * 4.0).clamp(0.0, 1023.0) as u16;
                        if v != s.pot {
                            s.pot = v;
                            s.turning[7] = 10;
                            send(Action::Adc { ch: 4, value: s.pot });
                        }
                    }
                    _ => {}
                }
                s.last_y = my;
            }
            if f.scroll != 0.0 {
                match skin::fm1_hit(kb, mx, my) {
                    Some(panel::Ctl::Enc(i)) => {
                        s.scroll_acc[i] += f.scroll;
                        while s.scroll_acc[i] >= 1.0 {
                            s.scroll_acc[i] -= 1.0;
                            turn(s, i, 1);
                        }
                        while s.scroll_acc[i] <= -1.0 {
                            s.scroll_acc[i] += 1.0;
                            turn(s, i, -1);
                        }
                    }
                    Some(panel::Ctl::Pot) => {
                        s.pot = (s.pot as f32 + f.scroll * 24.0).clamp(0.0, 1023.0) as u16;
                        s.turning[7] = 10;
                        send(Action::Adc { ch: 4, value: s.pot });
                    }
                    _ => {}
                }
            }
        }
        if !f.down {
            s.grab = None;
        }
        let mut want = [false; 41];
        for k in &f.keys {
            if let Some(id) = keys::fm1_note(layout, *k).or_else(|| keys::fm1_button(layout, *k)) {
                want[id as usize] = true;
            }
        }
        if let Some(panel::Ctl::Key(id) | panel::Ctl::Btn(id)) = s.grab {
            want[id as usize] = true;
        }
        for id in 0..41 {
            if want[id] != s.down[id] {
                s.down[id] = want[id];
                if want[id] && id >= 14 {
                    // the mouse's key at its height, the computer keyboard's at 100
                    let mouse = matches!(s.grab, Some(panel::Ctl::Key(k)) if k as usize == id);
                    send(Action::KeyVel { vel: if mouse { s.mouse_vel } else { 100 } });
                }
                send(Action::Key { id: id as u8, down: want[id] });
            }
        }
        for k in &f.pressed {
            if let Some((i, d)) = keys::encoder(layout, *k) {
                turn(s, i, d);
            }
        }
        for t in s.turning.iter_mut() {
            *t = t.saturating_sub(1);
        }
    }

    /// fm1-live's SMK-37 window loop, one frame.
    fn smk_frame(&mut self, f: &Frame) {
        use fm1_soc::smk::{ANALOG_MOD, ANALOG_PITCH, BTN0, PITCH_REST};
        use panel_smk::Ctl as S;
        let layout = self.settings.keys;
        let (Some(r), Input::Smk(s)) = (&self.running, &mut self.input) else { return };
        let send = |a: Action| r.session.send(a);
        let set_analog = |s: &mut SmkInput, i: usize, v: u16| {
            if s.analog[i] != v {
                s.analog[i] = v;
                send(Action::Adc { ch: i as u8, value: v });
            }
        };
        let turn = |s: &mut SmkInput, i: usize, clicks: i32| {
            s.enc_clicks[i] += clicks;
            s.turning[i] = 10;
            send(Action::Enc { enc: i as u8, clicks });
        };
        // 0 (softest) .. 1 (hardest) -> 80 ms .. 2 ms between a key's two contacts
        let vel = |v: f32| Action::Vel { us: (80_000f32.ln() + (2_000f32.ln() - 80_000f32.ln()) * v.clamp(0.0, 1.0)).exp() as u32 };
        if let Some((mx, my)) = f.pointer {
            if f.down && !f.was_down {
                s.grab = panel_smk::hit(mx, my);
                s.last_y = my;
                s.drag_acc = 0.0;
                match s.grab {
                    Some(S::Key(_, v)) => send(vel(v)),
                    Some(S::Pad(p)) => set_analog(s, p, 700),
                    _ => {}
                }
            }
            if f.down {
                match s.grab {
                    Some(S::Key(k, _)) => {
                        if let Some(S::Key(k2, v)) = panel_smk::hit(mx, my) {
                            if k2 != k {
                                send(vel(v));
                                s.grab = Some(S::Key(k2, v));
                            }
                        }
                    }
                    Some(S::Enc(i)) => {
                        s.drag_acc += s.last_y - my;
                        while s.drag_acc >= DRAG_PER_CLICK {
                            s.drag_acc -= DRAG_PER_CLICK;
                            turn(s, i, 1);
                        }
                        while s.drag_acc <= -DRAG_PER_CLICK {
                            s.drag_acc += DRAG_PER_CLICK;
                            turn(s, i, -1);
                        }
                    }
                    Some(S::Fader(i)) => set_analog(s, 16 + i, panel_smk::fader_value(i, my)),
                    Some(S::Pitch) => set_analog(s, ANALOG_PITCH as usize, panel_smk::wheel_value(true, my)),
                    Some(S::Mod) => set_analog(s, ANALOG_MOD as usize, panel_smk::wheel_value(false, my)),
                    _ => {}
                }
                s.last_y = my;
            }
            if f.scroll != 0.0 {
                if let Some(S::Enc(i)) = panel_smk::hit(mx, my) {
                    s.scroll_acc[i] += f.scroll;
                    while s.scroll_acc[i] >= 1.0 {
                        s.scroll_acc[i] -= 1.0;
                        turn(s, i, 1);
                    }
                    while s.scroll_acc[i] <= -1.0 {
                        s.scroll_acc[i] += 1.0;
                        turn(s, i, -1);
                    }
                }
            }
        }
        if !f.down {
            match s.grab {
                Some(S::Pad(p)) => set_analog(s, p, 0),
                Some(S::Pitch) => set_analog(s, ANALOG_PITCH as usize, PITCH_REST),
                _ => {}
            }
            s.grab = None;
        }
        let mut want = [false; 64];
        let mut pad_keys = [false; 8];
        for k in &f.keys {
            if let Some(id) = keys::smk_note(layout, *k) {
                want[id as usize] = true;
            }
            if let Some(b) = keys::smk_button(layout, *k) {
                want[(BTN0 + b) as usize] = true;
            }
            if let Some(p) = keys::smk_pad(layout, *k) {
                pad_keys[p] = true;
            }
        }
        for (p, &on) in pad_keys.iter().enumerate() {
            if s.grab != Some(S::Pad(p)) {
                set_analog(s, p, if on { 700 } else { 0 });
            }
        }
        if let Some(S::Key(id, _)) | Some(S::Btn(id)) = s.grab {
            want[id as usize] = true;
        }
        for id in 0..55 {
            if want[id] != s.down[id] {
                if want[id] && id < 37 && !matches!(s.grab, Some(S::Key(k, _)) if k as usize == id) {
                    send(vel(0.75));
                }
                s.down[id] = want[id];
                send(Action::Key { id: id as u8, down: want[id] });
            }
        }
        for k in &f.pressed {
            if let Some((i, d)) = keys::encoder(layout, *k) {
                turn(s, i, d);
            }
        }
        for t in s.turning.iter_mut() {
            *t = t.saturating_sub(1);
        }
    }

    /// Draw the panel (fm1-live's drawings) into the texture.
    fn draw_panel(&mut self, ctx: &egui::Context) {
        let (w, h) = self.panel_size();
        if self.buf.len() != w * h {
            self.buf = vec![0u32; w * h];
        }
        let Some(r) = &self.running else {
            self.fb.iter_mut().for_each(|p| *p = 0);
            return self.upload(ctx, w, h);
        };
        self.fb.copy_from_slice(&r.session.shared.lcd.lock().unwrap());
        let leds = *r.session.shared.leds.lock().unwrap();
        match &mut self.input {
            Input::Fm1(s) => {
                // (LEDs fade out over a few frames instead of flickering)
                for i in 0..41 {
                    s.shown_leds[i] = leds[i].max(s.shown_leds[i] * 0.8);
                }
                let st = panel::State { lcd: &self.fb, enc_clicks: &s.enc_clicks, pot: s.pot, down: &s.down, leds: &s.shown_leds, turning: &s.turning };
                let (t, k) = (self.settings.theme_fm1, self.settings.keybed);
                skin::fm1_draw(&mut self.buf, base_fm1(t, k), &st, t, k);
            }
            Input::Smk(s) => {
                use fm1_soc::smk::{ANALOG_MOD, ANALOG_PITCH};
                let pads = *r.session.shared.pads.lock().unwrap();
                let st = panel_smk::State {
                    lcd: &self.fb,
                    enc_clicks: &s.enc_clicks,
                    turning: &s.turning,
                    down: &s.down,
                    leds: &std::array::from_fn(|k| leds[k]),
                    pads: &pads,
                    pad_force: &std::array::from_fn(|p| s.analog[p]),
                    faders: &std::array::from_fn(|f| s.analog[16 + f]),
                    pitch: s.analog[ANALOG_PITCH as usize],
                    modw: s.analog[ANALOG_MOD as usize],
                };
                let t = self.settings.theme_smk;
                skin::smk_draw(&mut self.buf, base_smk(t), &st, t);
            }
        }
        self.upload(ctx, w, h);
    }

    fn upload(&mut self, ctx: &egui::Context, w: usize, h: usize) {
        let pixels = self.buf.iter().map(|&p| Color32::from_rgb((p >> 16) as u8, (p >> 8) as u8, p as u8)).collect();
        let img = egui::ColorImage { size: [w, h], pixels };
        match &mut self.tex {
            Some(t) => t.set(img, egui::TextureOptions::LINEAR),
            None => self.tex = Some(ctx.load_texture("panel", img, egui::TextureOptions::LINEAR)),
        }
    }

    // -------------------------------------------------------------- the picker

    fn picker_ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Firmware").size(26.0).strong());
            ui.add_space(16.0);
            for b in BoardId::ALL {
                let n = self.lists[&b].cat.firmwares.len();
                let t = RichText::new(format!("{}  ·  {n}", b.name())).size(18.0);
                if ui.selectable_label(self.tab == b, t).clicked() {
                    self.tab = b;
                }
            }
            ui.add_space(16.0);
            let r = ui.add(egui::TextEdit::singleline(&mut self.search).hint_text("Search name, author, description").desired_width(320.0));
            let nothing_focused = ui.ctx().memory(|m| m.focused().is_none());
            if self.focus_search || nothing_focused {
                r.request_focus();
                self.focus_search = false;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new(concat!("v", env!("CARGO_PKG_VERSION"))).weak()).on_hover_text(format!("Files: {}", self.store.root.display()));
                if self.running.is_some() && ui.button(RichText::new("Back to playing  (Esc)").size(16.0)).clicked() {
                    self.picker = false;
                }
                if ui.button("Open own .fwsc…").on_hover_text("A package from your computer: M-VAVE's own FM-1.fwsc or SMK-37 Pro_016.fwsc, or any firmware").clicked() {
                    if let Some(p) = rfd::FileDialog::new().add_filter("Firmware package", &["fwsc"]).pick_file() {
                        self.play_file(p);
                    }
                }
            });
        });
        ui.add_space(4.0);
        // the list's source and its updates
        let b = self.tab;
        let updates = self.updates(b);
        ui.horizontal(|ui| {
            let l = &self.lists[&b];
            let src = match (&l.source, l.fetching) {
                (_, true) => "checking upstream's list…".to_string(),
                (Source::Fetched, _) => "list: upstream's, just now".to_string(),
                (Source::Cached, _) => "list: upstream's, as last fetched (offline?)".to_string(),
                (Source::BuiltIn, _) => "list: built in (offline?)".to_string(),
            };
            ui.label(RichText::new(src).weak());
            if let Some(e) = &l.fetch_err {
                ui.label(RichText::new("⚠").color(Color32::from_rgb(230, 170, 60))).on_hover_text(e);
            }
            if !l.fetching && ui.small_button("Check again").clicked() {
                self.refetch(&ctx);
            }
            if !updates.is_empty() {
                ui.add_space(12.0);
                let t = format!("{} update{} available — update all", updates.len(), if updates.len() == 1 { "" } else { "s" });
                if ui.button(RichText::new(t).color(Color32::from_rgb(120, 200, 255))).clicked() {
                    for e in &updates {
                        self.download(b, e, false);
                    }
                }
            }
        });
        ui.add_space(6.0);
        ui.separator();

        let q = self.search.to_lowercase();
        let entries: Vec<Entry> = self.lists[&b]
            .cat
            .firmwares
            .iter()
            .filter(|e| q.is_empty() || [&e.name, &e.author, &e.about, &e.id].iter().any(|s| s.to_lowercase().contains(&q)))
            .cloned()
            .collect();
        let default_id = self.lists[&b].cat.default.clone();
        let mut chosen: Option<(Entry, bool)> = None; // (entry, download first)
        let mut get: Option<Entry> = None;
        egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
            ui.set_max_width(ui.available_width().min(1100.0));
            for e in &entries {
                let l = &self.lists[&b];
                let status = self.store.status(b, e, &l.locals);
                let key = (b, e.id.clone());
                let busy = self.busy.contains(&key);
                let playing = self.running.as_ref().is_some_and(|r| r.board == b && r.id == e.id);
                let fill = if playing { Color32::from_rgb(34, 52, 70) } else { Color32::from_rgb(36, 38, 42) };
                let resp = egui::Frame::new().fill(fill).corner_radius(8.0).inner_margin(egui::Margin::symmetric(14, 10)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&e.name).size(19.0).strong().color(Color32::WHITE));
                        if !e.version.is_empty() {
                            ui.label(RichText::new(&e.version).size(15.0).weak());
                        }
                        if !e.author.is_empty() {
                            ui.label(RichText::new(format!("by {}", e.author)).size(15.0).weak());
                        }
                        if playing {
                            ui.label(RichText::new("▶ playing").color(Color32::from_rgb(120, 200, 255)));
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if busy {
                                ui.label("downloading…");
                                ui.add(egui::Spinner::new());
                                return;
                            }
                            match &status {
                                Status::Ready => {
                                    if ui.button(RichText::new("Play").size(16.0)).clicked() {
                                        chosen = Some((e.clone(), false));
                                    }
                                    ui.label(RichText::new("✔ downloaded").color(Color32::from_rgb(110, 190, 120)));
                                }
                                Status::Missing => {
                                    if ui.button(RichText::new("Download & play").size(16.0)).clicked() {
                                        chosen = Some((e.clone(), true));
                                    }
                                }
                                Status::Update(v) => {
                                    if ui.button(RichText::new(format!("Update to {v} & play")).size(16.0)).clicked() {
                                        chosen = Some((e.clone(), true));
                                    }
                                    if ui.small_button("play the one here").clicked() {
                                        chosen = Some((e.clone(), false));
                                    }
                                }
                                Status::Own => {
                                    if ui.button(RichText::new("Open your file…").size(16.0)).clicked() {
                                        if let Some(p) = rfd::FileDialog::new().add_filter("Firmware package", &["fwsc"]).pick_file() {
                                            get = Some(Entry { id: p.to_string_lossy().into_owned(), ..e.clone() });
                                        }
                                    }
                                }
                            }
                        });
                    });
                    if !e.about.is_empty() {
                        ui.label(RichText::new(&e.about).size(14.5).color(Color32::from_gray(200)));
                    }
                    if !e.license.is_empty() || e.repo.is_some() || e.id == default_id {
                        ui.horizontal(|ui| {
                            if e.id == default_id {
                                ui.label(RichText::new("★ the web page's first pick").size(13.0).color(Color32::from_rgb(230, 190, 90)));
                            }
                            if !e.license.is_empty() {
                                ui.label(RichText::new(format!("licence: {}", e.license)).size(13.0).weak());
                            }
                            if let Some(repo) = &e.repo {
                                ui.hyperlink_to(RichText::new("source").size(13.0), repo);
                            }
                        });
                    }
                    if e.byo {
                        if let Some(d) = &e.download {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("M-VAVE's own: download it from M-VAVE, then open it here.").size(13.5).weak());
                                ui.hyperlink_to("M-VAVE's file", d);
                            });
                        }
                    }
                    if let Some(err) = self.errors.get(&key) {
                        ui.label(RichText::new(format!("Download failed: {err}")).size(13.5).color(Color32::from_rgb(240, 110, 100)));
                    }
                });
                // (the card itself plays it too)
                let card = ui.interact(resp.response.rect, ui.id().with(("card", &e.id)), egui::Sense::click());
                if card.clicked() && chosen.is_none() && !busy && !e.byo {
                    chosen = Some((e.clone(), !matches!(status, Status::Ready)));
                }
                ui.add_space(6.0);
            }
            if entries.is_empty() {
                ui.label(RichText::new("Nothing matches the search.").weak());
            }
        });
        if let Some(e) = get {
            self.play_file(PathBuf::from(e.id));
        }
        if let Some((e, fetch)) = chosen {
            if fetch {
                self.download(b, &e, true);
            } else {
                self.play_id(b, &e.id);
            }
        }
    }

    // -------------------------------------------------------------- the status bar

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.button(RichText::new("☰  Firmware  (F1)").size(15.0)).clicked() {
                self.open_picker();
            }
            if ui.selectable_label(self.settings.library, RichText::new("▤  Library  (F2)").size(15.0)).clicked() {
                self.toggle_library();
            }
            if let Some(r) = &self.running {
                ui.label(RichText::new(format!("{}  ·  {} {}", r.board.name(), r.name, r.version)).strong());
                let s = r.session.shared.stats.lock().unwrap().clone();
                if let Some(h) = &s.halted {
                    ui.label(RichText::new(format!("halted: {h}")).color(Color32::from_rgb(240, 110, 100)));
                } else if r.started.elapsed() > Duration::from_millis(700) {
                    let c = if s.speed >= 0.97 { Color32::from_gray(190) } else { Color32::from_rgb(240, 170, 80) };
                    ui.label(RichText::new(format!("{:.2}× real time   audio {:.0} ms   {} underruns", s.speed, s.buffered_ms, s.underruns)).color(c));
                }
            } else {
                ui.label(RichText::new("nothing playing").weak());
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.menu_button(RichText::new("⚙  Settings").size(15.0), |ui| self.settings_menu(ui));
                self.look_controls(ui);
                if self.settings.hints {
                    ui.label(RichText::new(format!("keys: {}", self.settings.keys.name())).weak());
                }
            });
        });
    }

    /// The colours of the board on show, and (FM-1) the keybed swap: pills or a piano keyboard.
    fn look_controls(&mut self, ui: &mut egui::Ui) {
        let smk = matches!(self.input, Input::Smk(_));
        let cur = if smk { self.settings.theme_smk } else { self.settings.theme_fm1 };
        let mut pick = cur;
        ui.menu_button(RichText::new(format!("🎨  {}", cur.short())).size(15.0), |ui| {
            ui.set_min_width(220.0);
            ui.label(RichText::new(format!("{} colours", if smk { "SMK-37" } else { "FM-1" })).strong());
            for t in Theme::ALL {
                ui.horizontal(|ui| {
                    let (body, keys) = t.swatch();
                    let c = |v: u32| Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8);
                    let (r, _) = ui.allocate_exact_size(egui::vec2(34.0, 18.0), egui::Sense::hover());
                    ui.painter().rect_filled(r, 4.0, c(body));
                    ui.painter().rect_filled(egui::Rect::from_min_size(r.min + egui::vec2(18.0, 4.0), egui::vec2(12.0, 10.0)), 3.0, c(keys));
                    if ui.radio_value(&mut pick, t, t.name()).clicked() {
                        ui.close_menu();
                    }
                });
            }
            ui.label(RichText::new("The real units come in these colours.").size(12.5).weak());
        });
        if pick != cur {
            if smk {
                self.settings.theme_smk = pick;
            } else {
                self.settings.theme_fm1 = pick;
            }
            self.board_tex.clear();
            self.settings.save(&self.store);
        }
        if !smk {
            let piano = self.settings.keybed == Keybed::Piano;
            let label = if piano { "🎹  Piano keys" } else { "💊  Pill keys" };
            if ui.button(RichText::new(label).size(15.0)).on_hover_text("Swap the FM-1's keybed: its pill keys, or a piano keyboard in the same groove").clicked() {
                self.release_all();
                self.settings.keybed = if piano { Keybed::Pills } else { Keybed::Piano };
                self.board_tex.clear();
                self.settings.save(&self.store);
            }
        }
    }

    /// Clock, MIDI input, the computer keyboard's layout and its letters on the keys.
    fn settings_menu(&mut self, ui: &mut egui::Ui) {
        ui.set_min_width(330.0);
        ui.label(RichText::new("Computer keyboard").strong());
        let mut layout = self.settings.keys;
        ui.radio_value(&mut layout, Layout::Ableton, "Ableton: A S D F … white, W E T … black (from C4)")
            .on_hover_text("Z / X octave, Space PLAY, R REC, 1…0 the buttons (SMK-37: 1…8 the pads), arrows and , . the encoders");
        ui.radio_value(&mut layout, Layout::Tracker, "FL Studio / tracker: Z X C V … and Q W E R …, two octaves")
            .on_hover_text("- / = octave, Space PLAY, Enter REC, F3…F12 the buttons (SMK-37: F3…F10 the pads), arrows and Page Up / Down the encoders");
        let mut hints = self.settings.hints;
        ui.checkbox(&mut hints, "Show the letters on the keys");
        if layout != self.settings.keys || hints != self.settings.hints {
            self.release_all();
            self.settings.keys = layout;
            self.settings.hints = hints;
            self.settings.save(&self.store);
        }
        ui.separator();
        ui.label(RichText::new("Clock").strong());
        let mut mhz = self.settings.mhz;
        ui.horizontal_wrapped(|ui| {
            for c in CLOCKS {
                ui.radio_value(&mut mhz, c, format!("{c} MHz"));
            }
        });
        ui.label(RichText::new("240 MHz is the real chip; lower it if the sound crackles.").size(12.5).weak());
        if mhz != self.settings.mhz {
            self.settings.mhz = mhz;
            self.settings.save(&self.store);
            self.restart();
        }
        ui.separator();
        ui.label(RichText::new("MIDI input").strong());
        let mut choice = self.settings.midi.clone();
        ui.radio_value(&mut choice, "all".to_string(), "all inputs");
        ui.radio_value(&mut choice, "none".to_string(), "none");
        for p in &self.midi_ports {
            ui.radio_value(&mut choice, p.clone(), p);
        }
        if ui.small_button("look for inputs again").clicked() {
            self.midi_ports = Midi::list();
            self.midi.connect(&self.settings.midi);
        }
        if choice != self.settings.midi {
            // (the inputs feed whatever plays: the running synth stays plugged in)
            self.settings.midi = choice.clone();
            self.midi.connect(&choice);
            self.settings.save(&self.store);
        }
    }

    /// The running firmware again (a new clock or MIDI choice); its flash is kept across.
    fn restart(&mut self) {
        if let Some(r) = &self.running {
            let (b, id) = (r.board, r.id.clone());
            self.play_id(b, &id);
        }
    }

    /// The window as wide as it is, as high as the board's panel wants (and the status bar).
    fn fit_window(&self) {
        let (w, h) = self.panel_size();
        let width = self.ctx.input(|i| i.viewport().inner_rect.map(|r| r.width())).unwrap_or(1280.0);
        let strip = if self.settings.library { LIBRARY_W } else { 0.0 };
        let height = ((width - strip).max(320.0) * h as f32 / w as f32 + 38.0).round();
        self.ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(width, height)));
    }

    /// Let go of every key, button and pad the window holds (the picker takes the keyboard).
    fn release_all(&mut self) {
        let Some(r) = &self.running else { return };
        match &mut self.input {
            Input::Fm1(s) => {
                for id in 0..41 {
                    if s.down[id] {
                        s.down[id] = false;
                        r.session.send(Action::Key { id: id as u8, down: false });
                    }
                }
                s.grab = None;
            }
            Input::Smk(s) => {
                for id in 0..64 {
                    if s.down[id] {
                        s.down[id] = false;
                        r.session.send(Action::Key { id: id as u8, down: false });
                    }
                }
                for p in 0..16 {
                    if s.analog[p] != 0 {
                        s.analog[p] = 0;
                        r.session.send(Action::Adc { ch: p as u8, value: 0 });
                    }
                }
                s.grab = None;
            }
        }
    }

    fn toggle_library(&mut self) {
        self.settings.library = !self.settings.library;
        self.settings.save(&self.store);
    }

    /// The computer key's letters on each note key of the panel, on a small dark chip.
    fn key_letters(&self, p: &egui::Painter, rect: egui::Rect) {
        let (w, h) = self.panel_size();
        let smk = matches!(self.input, Input::Smk(_));
        let s = rect.width() / w as f32;
        let _ = h; // (the panel keeps its aspect: one scale for both)
        let list = if smk { panel_smk::keys() } else { skin::fm1_keys(self.settings.keybed) };
        let font = egui::FontId::monospace((15.0 * s * if smk { 1.2 } else { 1.4 }).max(9.0));
        for (id, x, y, kw, kh, black) in list {
            let t = keys::label(self.settings.keys, smk, id);
            if t.is_empty() {
                continue;
            }
            let g = p.layout_no_wrap(t, font.clone(), Color32::WHITE);
            let cx = rect.min.x + (x as f32 + kw as f32 / 2.0) * s;
            let by = rect.min.y + (y as f32 + kh as f32 * if black { 0.78 } else { 0.86 }) * s;
            let r = egui::Rect::from_center_size(egui::pos2(cx, by), g.size() + egui::vec2(8.0, 3.0));
            p.rect_filled(r, 4.0, Color32::from_black_alpha(170));
            p.galley(r.min + egui::vec2(4.0, 1.5), g, Color32::WHITE);
        }
    }

    /// The playing firmware's screen kept for its Library tile.
    fn snapshot(&mut self, r: &Running) {
        let lcd = r.session.shared.lcd.lock().unwrap().clone();
        if lcd.iter().all(|&p| p == 0) {
            return;
        }
        let path = self.store.snap_path(r.board, &r.id.replace("own:", "own-"));
        if let Some(d) = path.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let bytes: Vec<u8> = lcd.iter().flat_map(|p| p.to_le_bytes()).collect();
        let _ = std::fs::write(&path, bytes);
        self.thumbs.remove(&format!("{}/{}", r.board.dir(), r.id));
    }

    /// A 240 x 240 RGB565 screen as an image.
    fn lcd_image(lcd: &[u16]) -> egui::ColorImage {
        let pixels = lcd
            .iter()
            .map(|&p| {
                let p = p as u32;
                Color32::from_rgb(((p >> 11) * 255 / 31) as u8, (((p >> 5) & 63) * 255 / 63) as u8, ((p & 31) * 255 / 31) as u8)
            })
            .collect();
        egui::ColorImage { size: [240, 240], pixels }
    }

    /// The strip on the left: both boards, and the firmwares on this computer as tiles of their
    /// screens (the playing one live), the last played first.
    fn library_ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        // the playing firmware's screen, live (ten times a second)
        if self.running.is_some() && self.live_at.elapsed() > Duration::from_millis(100) {
            let img = Self::lcd_image(&self.fb);
            match &mut self.live_tex {
                Some(t) => t.set(img, egui::TextureOptions::LINEAR),
                None => self.live_tex = Some(ctx.load_texture("live", img, egui::TextureOptions::LINEAR)),
            }
            self.live_at = Instant::now();
        }
        ui.label(RichText::new("Library").size(20.0).strong());
        ui.add_space(4.0);
        let mut play: Option<(BoardId, String)> = None;
        egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
            for b in BoardId::ALL {
                // the board, as its panel looks
                let tex = self.board_tex.entry(b).or_insert_with(|| {
                    // (the panel at rest: its knobs, keys and a dark screen)
                    let lcd = vec![0u16; 240 * 240];
                    let (w, h, buf) = match b {
                        BoardId::Fm1 => {
                            let mut buf = vec![0u32; panel::W * panel::H];
                            let st = panel::State { lcd: &lcd, enc_clicks: &[0; 7], pot: 600, down: &[false; 41], leds: &[0.0; 41], turning: &[0; 8] };
                            let (t, k) = (self.settings.theme_fm1, self.settings.keybed);
                            skin::fm1_draw(&mut buf, base_fm1(t, k), &st, t, k);
                            (panel::W, panel::H, buf)
                        }
                        BoardId::Smk37 => {
                            use fm1_soc::smk::{MOD_REST, PITCH_REST};
                            let mut buf = vec![0u32; panel_smk::W * panel_smk::H];
                            let st = panel_smk::State {
                                lcd: &lcd,
                                enc_clicks: &[0; 8],
                                turning: &[0; 8],
                                down: &[false; 64],
                                leds: &std::array::from_fn(|_| 0.0),
                                pads: &[0; 16],
                                pad_force: &std::array::from_fn(|_| 0),
                                faders: &std::array::from_fn(|_| 0),
                                pitch: PITCH_REST,
                                modw: MOD_REST,
                            };
                            let t = self.settings.theme_smk;
                            skin::smk_draw(&mut buf, base_smk(t), &st, t);
                            (panel_smk::W, panel_smk::H, buf)
                        }
                    };
                    let pixels = buf.iter().map(|&p| Color32::from_rgb((p >> 16) as u8, (p >> 8) as u8, p as u8)).collect();
                    ctx.load_texture(format!("board-{}", b.dir()), egui::ColorImage { size: [w, h], pixels }, egui::TextureOptions::LINEAR)
                });
                let aspect = tex.size()[1] as f32 / tex.size()[0] as f32;
                let width = ui.available_width();
                ui.add(egui::Image::new((tex.id(), egui::vec2(width, width * aspect))).corner_radius(6.0));
                ui.label(RichText::new(b.name()).size(16.0).strong());
                // its firmwares here, the last played first
                let l = &self.lists[&b];
                let mut ids: Vec<(usize, String, String, String)> = l
                    .locals
                    .iter()
                    .map(|(id, loc)| {
                        let key = format!("{}/{id}", b.dir());
                        let rank = self.settings.recent.iter().position(|k| *k == key).unwrap_or(usize::MAX);
                        let name = l.cat.firmwares.iter().find(|e| &e.id == id).map(|e| e.name.clone()).unwrap_or_else(|| loc.name.clone());
                        (rank, name, loc.version.clone(), id.clone())
                    })
                    .collect();
                ids.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.to_lowercase().cmp(&b.1.to_lowercase())));
                if ids.is_empty() {
                    ui.label(RichText::new("none downloaded yet (F1)").size(13.0).weak());
                }
                for (_, name, version, id) in ids {
                    let key = format!("{}/{id}", b.dir());
                    let playing = self.running.as_ref().is_some_and(|r| r.board == b && r.id == id);
                    let thumb = if playing {
                        self.live_tex.as_ref().map(|t| t.id())
                    } else {
                        let path = self.store.snap_path(b, &id);
                        self.thumbs
                            .entry(key.clone())
                            .or_insert_with(|| {
                                let bytes = std::fs::read(&path).ok().filter(|v| v.len() == 240 * 240 * 2)?;
                                let lcd: Vec<u16> = bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
                                Some(ctx.load_texture(format!("thumb-{key}"), Self::lcd_image(&lcd), egui::TextureOptions::LINEAR))
                            })
                            .as_ref()
                            .map(|t| t.id())
                    };
                    let fill = if playing { Color32::from_rgb(34, 52, 70) } else { Color32::from_rgb(32, 34, 38) };
                    let resp = egui::Frame::new().fill(fill).corner_radius(6.0).inner_margin(egui::Margin::same(5)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            let size = egui::vec2(52.0, 52.0);
                            match thumb {
                                Some(t) => {
                                    ui.add(egui::Image::new((t, size)).corner_radius(4.0));
                                }
                                None => {
                                    // (never played here yet: its initials)
                                    let (r, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                                    ui.painter().rect_filled(r, 4.0, Color32::from_rgb(52, 56, 64));
                                    let ini: String = name.chars().filter(|c| c.is_alphanumeric()).take(2).collect();
                                    ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, ini, egui::FontId::proportional(18.0), Color32::from_gray(200));
                                }
                            }
                            ui.vertical(|ui| {
                                ui.label(RichText::new(&name).size(14.5).strong().color(Color32::WHITE));
                                let v = if playing { format!("{version}  ▶") } else { version.clone() };
                                ui.label(RichText::new(v).size(12.5).weak());
                            });
                        });
                    });
                    let click = ui.interact(resp.response.rect, ui.id().with(("lib", &key)), egui::Sense::click());
                    if click.clicked() && !playing {
                        play = Some((b, id.clone()));
                    }
                    if click.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    ui.add_space(3.0);
                }
                ui.add_space(12.0);
            }
            if ui.button("More firmwares…  (F1)").clicked() {
                self.open_picker();
            }
        });
        if let Some((b, id)) = play {
            self.play_id(b, &id);
        }
    }

    fn open_picker(&mut self) {
        self.release_all();
        self.picker = true;
        self.focus_search = true;
        if let Some(r) = &self.running {
            self.tab = r.board;
        }
    }
}


/// A panel's still parts, drawn once per theme (and keybed) and kept.
fn base_fm1(t: Theme, k: Keybed) -> &'static [u32] {
    static B: std::sync::Mutex<Option<HashMap<(Theme, Keybed), &'static [u32]>>> = std::sync::Mutex::new(None);
    let mut m = B.lock().unwrap();
    m.get_or_insert_with(HashMap::new).entry((t, k)).or_insert_with(|| skin::fm1_base(t, k).leak())
}

fn base_smk(t: Theme) -> &'static [u32] {
    static B: std::sync::Mutex<Option<HashMap<Theme, &'static [u32]>>> = std::sync::Mutex::new(None);
    let mut m = B.lock().unwrap();
    m.get_or_insert_with(HashMap::new).entry(t).or_insert_with(|| skin::smk_base(t).leak())
}

impl eframe::App for Desk {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_jobs();
        // F1 opens the picker, Esc closes it (back to what plays)
        let (f1, esc, f2) = ctx.input(|i| (i.key_pressed(Key::F1), i.key_pressed(Key::Escape), i.key_pressed(Key::F2)));
        if f2 && !self.picker {
            self.toggle_library();
        }
        if f1 && !self.picker {
            self.open_picker();
        } else if (esc || f1) && self.picker && self.running.is_some() {
            self.picker = false;
        }

        let title = match &self.running {
            Some(r) => format!("{TITLE} — {} · {}", r.board.name(), r.name),
            None => TITLE.to_string(),
        };
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }

        if !self.picker && self.settings.library {
            egui::SidePanel::left("library")
                .exact_width(LIBRARY_W)
                .resizable(false)
                .frame(egui::Frame::new().fill(Color32::from_rgb(22, 23, 26)).inner_margin(egui::Margin::symmetric(10, 8)))
                .show(ctx, |ui| self.library_ui(ui));
        }
        if !self.picker {
            egui::TopBottomPanel::bottom("status").frame(egui::Frame::new().fill(Color32::from_rgb(24, 25, 28)).inner_margin(egui::Margin::symmetric(10, 6))).show(ctx, |ui| self.status_bar(ui));
        }
        egui::CentralPanel::default().frame(egui::Frame::new().fill(Color32::from_rgb(16, 17, 19))).show(ctx, |ui| {
            let avail = ui.max_rect();
            let rect = self.panel_rect(avail);
            if !self.picker {
                // playing: the synth gets the keyboard and the mouse
                let typing = ctx.memory(|m| m.focused().is_some());
                let f = self.read_input(ctx, rect, !typing);
                match self.input {
                    Input::Fm1(_) => self.fm1_frame(&f),
                    Input::Smk(_) => self.smk_frame(&f),
                }
                self.draw_panel(ctx);
            } else if self.tex.is_none() {
                self.draw_panel(ctx);
            }
            if let Some(t) = &self.tex {
                ui.painter().image(t.id(), rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
            }
            if !self.picker && self.settings.hints && self.running.is_some() {
                self.key_letters(ui.painter(), rect);
            }
            if self.picker {
                // over the panel, dimmed, full window
                ui.painter().rect_filled(avail, 0.0, Color32::from_black_alpha(215));
                let inner = avail.shrink2(egui::vec2(28.0, 18.0));
                ui.allocate_new_ui(egui::UiBuilder::new().max_rect(inner), |ui| self.picker_ui(ui));
            }
            if let Some((m, at)) = &self.message {
                if at.elapsed() < Duration::from_secs(8) {
                    let p = avail.center_bottom() - egui::vec2(0.0, 36.0);
                    let g = ui.painter().layout_no_wrap(m.clone(), egui::FontId::proportional(16.0), Color32::WHITE);
                    let r = egui::Rect::from_center_size(p, g.size() + egui::vec2(24.0, 14.0));
                    ui.painter().rect_filled(r, 6.0, Color32::from_rgb(120, 40, 36));
                    ui.painter().galley(r.min + egui::vec2(12.0, 7.0), g, Color32::WHITE);
                }
            }
        });
        // a playing panel animates; the picker repaints on input (and when a job comes in)
        if !self.picker {
            ctx.request_repaint();
        } else if !self.busy.is_empty() || self.lists.values().any(|l| l.fetching) {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if let Some(r) = self.running.take() {
            self.snapshot(&r);
            r.session.stop(&self.midi);
        }
        self.settings.save(&self.store);
    }
}
