//! The window: the playing synth's panel (fm1-live's drawings and mouse / keyboard handling),
//! a slim status bar, and the firmware picker over the panel, full window.

use crate::catalog::{BoardId, Catalog, Entry, Local, Source, Status, Store};
use crate::engine::{self, Audio, Midi, Session};
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
}

impl Default for Settings {
    fn default() -> Self {
        Settings { board: BoardId::Fm1, last: BTreeMap::new(), own: BTreeMap::new(), mhz: 240, midi: "all".into() }
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
        Fm1Input { down: [false; 41], enc_clicks: [0; 7], turning: [0; 8], scroll_acc: [0.0; 8], shown_leds: [0.0; 41], grab: None, last_y: 0.0, drag_acc: 0.0, pot }
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
        let (Some(r), Input::Fm1(s)) = (&self.running, &mut self.input) else { return };
        let send = |a: Action| r.session.send(a);
        let turn = |s: &mut Fm1Input, i: usize, clicks: i32| {
            s.enc_clicks[i] += clicks;
            s.turning[i] = 10;
            send(Action::Enc { enc: panel::ENCS[i].1, clicks });
        };
        if let Some((mx, my)) = f.pointer {
            if f.down && !f.was_down {
                s.grab = panel::hit(mx, my);
                s.last_y = my;
                s.drag_acc = 0.0;
            } else if f.down {
                match s.grab {
                    Some(panel::Ctl::Key(_)) => {
                        if let Some(panel::Ctl::Key(k)) = panel::hit(mx, my) {
                            s.grab = Some(panel::Ctl::Key(k));
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
                match panel::hit(mx, my) {
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
            if let Some(id) = piano(*k).or_else(|| button(*k)) {
                want[id as usize] = true;
            }
        }
        if let Some(panel::Ctl::Key(id) | panel::Ctl::Btn(id)) = s.grab {
            want[id as usize] = true;
        }
        for id in 0..41 {
            if want[id] != s.down[id] {
                s.down[id] = want[id];
                send(Action::Key { id: id as u8, down: want[id] });
            }
        }
        for k in &f.pressed {
            match k {
                Key::ArrowLeft => turn(s, 0, -1),
                Key::ArrowRight => turn(s, 0, 1),
                Key::ArrowDown => turn(s, 1, -1),
                Key::ArrowUp => turn(s, 1, 1),
                Key::Comma => turn(s, 2, -1),
                Key::Period => turn(s, 2, 1),
                _ => {}
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
            if let Some(id) = piano(*k) {
                want[(id - 21 + 12) as usize] = true; // (piano() gives the FM-1's ids, C4 = 21)
            }
            let b = match k {
                Key::Z => Some(4),
                Key::X => Some(5),
                Key::Space => Some(16),
                Key::R => Some(17),
                _ => None,
            };
            if let Some(b) = b {
                want[(BTN0 + b) as usize] = true;
            }
            if let Some(p) = [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5, Key::Num6, Key::Num7, Key::Num8].iter().position(|p| p == k) {
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
            match k {
                Key::ArrowLeft => turn(s, 0, -1),
                Key::ArrowRight => turn(s, 0, 1),
                Key::ArrowDown => turn(s, 1, -1),
                Key::ArrowUp => turn(s, 1, 1),
                Key::Comma => turn(s, 2, -1),
                Key::Period => turn(s, 2, 1),
                _ => {}
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
                panel::draw(&mut self.buf, base_fm1(), &st);
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
                panel_smk::draw(&mut self.buf, base_smk(), &st);
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
                // MIDI in
                let mut choice = self.settings.midi.clone();
                egui::ComboBox::from_id_salt("midi").selected_text(format!("MIDI: {}", short(&choice))).show_ui(ui, |ui| {
                    ui.selectable_value(&mut choice, "all".to_string(), "all inputs");
                    ui.selectable_value(&mut choice, "none".to_string(), "none");
                    for p in &self.midi_ports {
                        ui.selectable_value(&mut choice, p.clone(), p);
                    }
                });
                if choice != self.settings.midi {
                    // (the inputs feed whatever plays: the running synth stays plugged in)
                    self.settings.midi = choice.clone();
                    self.midi.connect(&choice);
                    self.settings.save(&self.store);
                }
                // clock
                let mut mhz = self.settings.mhz;
                egui::ComboBox::from_id_salt("clock").selected_text(format!("{mhz} MHz")).show_ui(ui, |ui| {
                    for c in CLOCKS {
                        let label = if c == 240 { "240 MHz (the real chip)".to_string() } else { format!("{c} MHz") };
                        ui.selectable_value(&mut mhz, c, label);
                    }
                });
                if mhz != self.settings.mhz {
                    self.settings.mhz = mhz;
                    self.settings.save(&self.store);
                    self.restart();
                }
            });
        });
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
        let height = (width * h as f32 / w as f32 + 38.0).round();
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

    fn open_picker(&mut self) {
        self.release_all();
        self.picker = true;
        self.focus_search = true;
        if let Some(r) = &self.running {
            self.tab = r.board;
        }
    }
}

fn short(s: &str) -> String {
    if s.chars().count() > 18 { format!("{}…", s.chars().take(17).collect::<String>()) } else { s.to_string() }
}

/// fm1-live's `piano`: A W S E D F T G Y H U J K O L P ; ' = C4..F5 (the FM-1's matrix ids).
fn piano(k: Key) -> Option<u8> {
    let semis = match k {
        Key::A => 0,
        Key::W => 1,
        Key::S => 2,
        Key::E => 3,
        Key::D => 4,
        Key::F => 5,
        Key::T => 6,
        Key::G => 7,
        Key::Y => 8,
        Key::H => 9,
        Key::U => 10,
        Key::J => 11,
        Key::K => 12,
        Key::O => 13,
        Key::L => 14,
        Key::P => 15,
        Key::Semicolon => 16,
        Key::Quote => 17,
        _ => return None,
    };
    Some(21 + semis)
}

/// fm1-live's `button`: Z X OCTAVE, Space PLAY, R REC, 1..0 the button row.
fn button(k: Key) -> Option<u8> {
    Some(match k {
        Key::Z => 0,
        Key::X => 1,
        Key::Space => 12,
        Key::R => 13,
        Key::Num1 => 2,
        Key::Num2 => 3,
        Key::Num3 => 4,
        Key::Num4 => 5,
        Key::Num5 => 6,
        Key::Num6 => 7,
        Key::Num7 => 8,
        Key::Num8 => 9,
        Key::Num9 => 10,
        Key::Num0 => 11,
        _ => return None,
    })
}

fn base_fm1() -> &'static [u32] {
    static B: std::sync::OnceLock<Vec<u32>> = std::sync::OnceLock::new();
    B.get_or_init(panel::base)
}

fn base_smk() -> &'static [u32] {
    static B: std::sync::OnceLock<Vec<u32>> = std::sync::OnceLock::new();
    B.get_or_init(panel_smk::base)
}

impl eframe::App for Desk {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_jobs();
        // F1 opens the picker, Esc closes it (back to what plays)
        let (f1, esc) = ctx.input(|i| (i.key_pressed(Key::F1), i.key_pressed(Key::Escape)));
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
            r.session.stop(&self.midi);
        }
        self.settings.save(&self.store);
    }
}
