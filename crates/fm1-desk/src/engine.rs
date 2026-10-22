//! The emulated synth running: a firmware package booted on its board (FM-1 or SMK-37, as the
//! package says), on its own thread, paced by the audio device as fm1-live does. One audio stream
//! and the MIDI inputs live for the whole app; a session plugs into them while it runs. The
//! flash is kept per firmware: restored at start, written back when the session stops.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use fm1_core::{Bus, Cpu};
use fm1_soc::{machine, Action, Board, Soc};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

const RAM_BASE: u32 = 0x01c0_0000;
const RAM_SIZE: usize = 1 << 20;
/// How far emulated time runs ahead of the wall clock before audio starts (fm1-live's).
const LEAD_MS: f64 = 60.0;
/// Once the firmware streams audio, the emulator runs whenever less than this is queued.
const AUDIO_TARGET_MS: f64 = 50.0;

// ------------------------------------------------------------------ audio, for the whole app

pub struct AudioRing {
    q: Mutex<VecDeque<i16>>,
    started: AtomicBool,
    underruns: AtomicU64,
}

impl AudioRing {
    fn reset(&self) {
        self.q.lock().unwrap().clear();
        self.started.store(false, Ordering::Relaxed);
        self.underruns.store(0, Ordering::Relaxed);
    }
}

/// The default output device, fed from the ring (44.1 kHz stereo, resampled linearly to the
/// device's rate): fm1-live's `start_audio`.
pub struct Audio {
    pub ring: Arc<AudioRing>,
    _stream: Option<cpal::Stream>,
    pub error: Option<String>,
}

impl Audio {
    pub fn open() -> Audio {
        let ring = Arc::new(AudioRing { q: Mutex::new(VecDeque::new()), started: AtomicBool::new(false), underruns: AtomicU64::new(0) });
        match start_audio(ring.clone()) {
            Ok(s) => Audio { ring, _stream: Some(s), error: None },
            Err(e) => Audio { ring, _stream: None, error: Some(e) },
        }
    }
    pub fn ok(&self) -> bool {
        self._stream.is_some()
    }
}

fn start_audio(ring: Arc<AudioRing>) -> Result<cpal::Stream, String> {
    let host = cpal::default_host();
    let dev = host.default_output_device().ok_or("no audio output device")?;
    let cfg = dev.default_output_config().map_err(|e| e.to_string())?;
    if cfg.sample_format() != cpal::SampleFormat::F32 {
        return Err(format!("output format {:?} not handled (f32 only)", cfg.sample_format()));
    }
    let channels = cfg.channels() as usize;
    let step = 44_100.0 / cfg.sample_rate().0 as f64; // source frames per output frame
    let (mut pos, mut cur, mut next) = (0.0f64, (0.0f32, 0.0f32), (0.0f32, 0.0f32));
    let stream = dev
        .build_output_stream(
            &cfg.config(),
            move |out: &mut [f32], _| {
                let mut q = ring.q.lock().unwrap();
                let mut starved = false;
                for frame in out.chunks_mut(channels) {
                    pos += step;
                    while pos >= 1.0 {
                        pos -= 1.0;
                        cur = next;
                        next = match (q.pop_front(), q.pop_front()) {
                            (Some(l), Some(r)) => (l as f32 / 32768.0, r as f32 / 32768.0),
                            _ => {
                                starved = true;
                                (0.0, 0.0)
                            }
                        };
                    }
                    let f = pos as f32;
                    let (l, r) = (cur.0 + (next.0 - cur.0) * f, cur.1 + (next.1 - cur.1) * f);
                    for (c, s) in frame.iter_mut().enumerate() {
                        *s = if c % 2 == 0 { l } else { r };
                    }
                }
                if starved && ring.started.load(Ordering::Relaxed) {
                    ring.underruns.fetch_add(1, Ordering::Relaxed);
                }
            },
            |e| eprintln!("audio: {e}"),
            None,
        )
        .map_err(|e| e.to_string())?;
    stream.play().map_err(|e| e.to_string())?;
    Ok(stream)
}

// ------------------------------------------------------------------ MIDI, for the whole app

/// The MIDI inputs: their bytes go to the running session's MIDI IN jack (none running: dropped).
pub struct Midi {
    target: Arc<Mutex<Option<mpsc::Sender<Action>>>>,
    conns: Vec<midir::MidiInputConnection<()>>,
    pub names: Vec<String>,
    /// "all", "none", or part of an input's name
    pub choice: String,
}

impl Midi {
    pub fn new() -> Midi {
        Midi { target: Arc::new(Mutex::new(None)), conns: Vec::new(), names: Vec::new(), choice: String::new() }
    }

    /// The inputs there are now (for the menu).
    pub fn list() -> Vec<String> {
        let Ok(probe) = midir::MidiInput::new("fm1-desk") else { return Vec::new() };
        probe.ports().iter().filter_map(|p| probe.port_name(p).ok()).collect()
    }

    /// Connect to the inputs `choice` names (as fm1-live's --midi).
    pub fn connect(&mut self, choice: &str) {
        self.conns.clear();
        self.names.clear();
        self.choice = choice.to_string();
        if choice == "none" {
            return;
        }
        let Ok(probe) = midir::MidiInput::new("fm1-desk") else { return };
        for port in probe.ports() {
            let Ok(name) = probe.port_name(&port) else { continue };
            if choice != "all" && !name.to_lowercase().contains(&choice.to_lowercase()) {
                continue;
            }
            let Ok(mut input) = midir::MidiInput::new("fm1-desk") else { continue };
            input.ignore(midir::Ignore::None);
            let target = self.target.clone();
            let got = input.connect(&port, "fm1-desk in", move |_, msg, _| {
                if let Some(tx) = target.lock().unwrap().as_ref() {
                    for &b in msg {
                        let _ = tx.send(Action::Midi { b });
                    }
                }
            }, ());
            if let Ok(c) = got {
                self.names.push(name);
                self.conns.push(c);
            }
        }
    }

    fn plug(&self, tx: Option<mpsc::Sender<Action>>) {
        *self.target.lock().unwrap() = tx;
    }
}

// ------------------------------------------------------------------ the machine (fm1-live's)

struct Machine {
    cpu: Cpu,
    cpu1: Option<Cpu>,
    soc: Soc,
    pending_tick: bool,
}

impl Machine {
    fn new(pkg: &[u8], flash: Option<&[u8]>, mhz: u64) -> Result<Machine, String> {
        let st = fm1_soc::stock_from_package(pkg, None)?;
        let mut soc = Soc::new(RAM_SIZE, st.xip.clone());
        soc.periph.hz = mhz * 1_000_000;
        soc.periph.skip_idle = true;
        let mut cpu = Cpu::new(st.entry);
        cpu.jit_on = std::env::var("FM1_NO_JIT").is_err();
        // the SPL hands cpu0 over in user mode; its boot-parameter struct (see fm1-live)
        cpu.user = true;
        let params = RAM_BASE + 0x7fc00;
        soc.write32(params, params + 0x40);
        cpu.regs[0] = fm1_soc::stock_boot(&mut soc, &st.flash, params, st.board);
        if let Some(f) = flash.filter(|f| f.len() == fm1_soc::FLASH_SIZE) {
            soc.restore_flash(f);
        }
        Ok(Machine { cpu, cpu1: None, soc, pending_tick: false })
    }

    fn run(&mut self, n: u32) -> Result<(), String> {
        let jit = self.cpu.jit_on;
        let Machine { cpu, cpu1, soc, pending_tick } = self;
        machine::run_dual(cpu1, soc, n, RAM_SIZE, jit, |soc, k| machine::run_blocks(cpu, soc, k, pending_tick))
    }
}

/// The board a package is for (its identity), without booting it.
pub fn board_of(pkg: &[u8]) -> Result<Board, String> {
    fm1_soc::stock_from_package(pkg, None).map(|s| s.board)
}

// ------------------------------------------------------------------ a session

/// What the window shows of a running synth (copied out by the emulator thread every 16 ms).
pub struct Shared {
    pub lcd: Mutex<Vec<u16>>,
    pub leds: Mutex<[f32; 41]>,
    pub pads: Mutex<[u32; 16]>,
    pub stats: Mutex<Stats>,
}

#[derive(Clone, Default)]
pub struct Stats {
    pub speed: f64,
    pub buffered_ms: f64,
    pub underruns: u64,
    pub halted: Option<String>,
    pub ms: u64,
}

pub struct Session {
    pub board: Board,
    /// the MASTER pot's position at boot (the FM-1's panel shows it)
    pub pot0: u16,
    pub tx: mpsc::Sender<Action>,
    pub shared: Arc<Shared>,
    quit: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Session {
    /// Boot `pkg` and run it until `stop`; its flash comes from (and goes back to) `save`.
    pub fn start(pkg: &[u8], save: PathBuf, mhz: u64, audio: &Audio, midi: &Midi) -> Result<Session, String> {
        let flash = std::fs::read(&save).ok();
        let mut m = Machine::new(pkg, flash.as_deref(), mhz)?;
        let board = m.soc.board;
        let pot0 = m.soc.periph.adc_value[4];
        let shared = Arc::new(Shared {
            lcd: Mutex::new(vec![0u16; 240 * 240]),
            leds: Mutex::new([0f32; 41]),
            pads: Mutex::new([0u32; 16]),
            stats: Mutex::new(Stats::default()),
        });
        let quit = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel::<Action>();
        audio.ring.reset();
        let (ring, audio_ok) = (audio.ring.clone(), audio.ok());
        let thread = {
            let (shared, quit) = (shared.clone(), quit.clone());
            std::thread::Builder::new().name("emulator".into()).spawn(move || {
                high_priority();
                run_loop(&mut m, &rx, &shared, &quit, &ring, audio_ok);
                save_flash(&m, &save);
            }).map_err(|e| e.to_string())?
        };
        midi.plug(Some(tx.clone()));
        Ok(Session { board, pot0, tx, shared, quit, thread: Some(thread) })
    }

    pub fn send(&self, a: Action) {
        let _ = self.tx.send(a);
    }

    /// Stop the emulator (its flash is written back) and unplug the MIDI inputs.
    pub fn stop(mut self, midi: &Midi) {
        midi.plug(None);
        self.quit.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.quit.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// fm1-live's emulator thread: paced to the audio device (or, before audio starts / without
/// one, to the wall clock), the screen and LEDs copied out every 16 ms, the speed every 500 ms.
fn run_loop(m: &mut Machine, rx: &mpsc::Receiver<Action>, shared: &Shared, quit: &AtomicBool, ring: &AudioRing, audio_ok: bool) {
    let start = Instant::now();
    let mut offset_ms = 0.0f64;
    let mut last_lcd = Instant::now();
    let mut last_stat = Instant::now();
    let (mut stat_emu, mut stat_wall) = (0u64, Instant::now());
    while !quit.load(Ordering::Relaxed) {
        while let Ok(a) = rx.try_recv() {
            m.soc.periph.apply(&a);
        }
        let real = start.elapsed().as_secs_f64() * 1000.0 - offset_ms;
        let emu_ms = m.soc.periph.cycles as f64 * 1000.0 / m.soc.periph.hz as f64;
        if real - emu_ms > 250.0 {
            offset_ms += real - emu_ms - LEAD_MS; // too far behind: resync, do not race
        }
        let buffered = ring.q.lock().unwrap().len() as f64 / 88.2;
        let behind = if audio_ok && m.soc.periph.audio_halves > 0 { buffered < AUDIO_TARGET_MS } else { emu_ms < real + LEAD_MS };
        if behind {
            if let Err(e) = m.run(20_000) {
                shared.stats.lock().unwrap().halted = Some(e);
                break;
            }
        } else {
            std::thread::sleep(Duration::from_millis(1));
        }
        if !m.soc.periph.audio.is_empty() {
            let mut q = ring.q.lock().unwrap();
            q.extend(m.soc.periph.audio.drain(..));
            ring.started.store(true, Ordering::Relaxed);
            let cap = 44_100 * 2 / 4; // 250 ms
            if q.len() > cap {
                let extra = q.len() - cap;
                q.drain(..extra);
            }
        }
        if last_lcd.elapsed() >= Duration::from_millis(16) {
            shared.lcd.lock().unwrap().copy_from_slice(&m.soc.lcd.fb);
            if m.soc.board == Board::Smk37 {
                let mut l = [0f32; 41];
                l[..fm1_soc::smk::NLED].copy_from_slice(&m.soc.periph.smk.led_take());
                *shared.leds.lock().unwrap() = l;
                *shared.pads.lock().unwrap() = m.soc.periph.smk.pads;
            } else {
                *shared.leds.lock().unwrap() = m.soc.periph.led_take();
            }
            last_lcd = Instant::now();
        }
        if last_stat.elapsed() >= Duration::from_millis(500) {
            let e = m.soc.periph.ms();
            let mut s = shared.stats.lock().unwrap();
            s.speed = (e - stat_emu) as f64 / stat_wall.elapsed().as_secs_f64() / 1000.0;
            s.buffered_ms = ring.q.lock().unwrap().len() as f64 / 88.2;
            s.underruns = ring.underruns.load(Ordering::Relaxed);
            s.ms = e;
            stat_emu = e;
            stat_wall = Instant::now();
            last_stat = Instant::now();
        }
    }
}

/// The flash back to its file when the firmware wrote to it (a new file renamed over the old:
/// a run killed mid-save leaves the last save whole).
fn save_flash(m: &Machine, path: &PathBuf) {
    if m.soc.flash.ops == 0 {
        return;
    }
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let tmp = path.with_extension("tmp");
    if let Err(e) = std::fs::write(&tmp, &m.soc.flash.mem).and_then(|_| std::fs::rename(&tmp, path)) {
        eprintln!("{}: {e}", path.display());
    }
}

/// The emulator thread above normal priority: fewer gaps in its audio when the PC is busy.
fn high_priority() {
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        extern "system" {
            fn GetCurrentThread() -> isize;
            fn SetThreadPriority(thread: isize, priority: i32) -> i32;
        }
        const THREAD_PRIORITY_ABOVE_NORMAL: i32 = 1;
        // SAFETY: the pseudo handle of the calling thread; failure only leaves the priority as is
        unsafe {
            SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_ABOVE_NORMAL);
        }
    }
}
