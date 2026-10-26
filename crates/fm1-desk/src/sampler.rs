//! The Sampler: a Desk add-on, not part of the FM-1 or SMK-37. A little 80s sampling keyboard
//! (in the spirit of the Casio SK-1 and Yamaha's VSS samplers) beside the emulated synth: record
//! a sound from the microphone / line in, or resample what the synth plays, or load a WAV; play
//! it from the panel's keys, the computer keyboard and MIDI, pitched by playback speed.
//!
//! "SK-1 grit" is how such a sampler sounds, done the way it happened there: the sound kept as
//! 8-bit numbers (256 levels: quiet parts turn grainy) taken 9,380 times a second with nothing
//! filtering out what that rate cannot hold (tones above half of it fold back down, metallic),
//! played back by reading those numbers faster or slower, each held until the next (the steps
//! add their own bright images). Pitch and length go together, as on tape. "Clean" keeps the
//! recording as it came and reads it smoothly.
//!
//! The length is a memory size: the SK-1 had 1.4 seconds; the longer ones are the cheat.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use fm1_soc::Action;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::{Arc, Mutex};

/// The SK-1's sampling rate.
pub const SK1_RATE: f32 = 9380.0;
const VOICES: usize = 8;
/// A recording that waits for sound starts at this level, with this much before it kept.
const TRIGGER: f32 = 0.03;
const PREROLL_S: f32 = 0.01;

/// Sample memory sizes, in seconds: the SK-1's, then the cheat.
pub const LENGTHS: [(f32, &str); 5] = [(1.4, "1.4 s  (SK-1)"), (3.0, "3 s"), (10.0, "10 s"), (30.0, "30 s"), (120.0, "2 min")];

/// What the note keys (panel, computer keyboard, MIDI) play.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, Default)]
pub enum Route {
    #[default]
    Synth,
    Sampler,
    Both,
}

/// Where a recording comes from.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, Default)]
pub enum Source {
    /// the computer's default input: a microphone, line in
    #[default]
    Input,
    /// what the emulated synth plays (resampling)
    Synth,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Params {
    pub route: Route,
    pub source: Source,
    /// the memory, in seconds
    pub length: f32,
    pub grit: bool,
    pub looped: bool,
    pub reverse: bool,
    /// a recording starts when sound comes
    pub wait: bool,
    /// octaves up / down from the recorded pitch (which sounds at C4)
    pub octave: i8,
    pub volume: f32,
    /// seconds a released key takes to fade
    pub release: f32,
}

impl Default for Params {
    fn default() -> Self {
        Params { route: Route::Synth, source: Source::Input, length: 1.4, grit: true, looped: false, reverse: false, wait: true, octave: 0, volume: 0.8, release: 0.12 }
    }
}

/// A sound in memory: as recorded, and as the SK-1 would have kept it.
pub struct Sample {
    pub raw: Vec<f32>,
    pub rate: f32,
    pub grit: Vec<f32>,
}

impl Sample {
    pub fn new(raw: Vec<f32>, rate: f32) -> Sample {
        let grit = crunch(&raw, rate);
        Sample { raw, rate, grit }
    }
    pub fn seconds(&self) -> f32 {
        self.raw.len() as f32 / self.rate
    }
    /// The data that plays, and its rate.
    pub fn data(&self, grit: bool) -> (&[f32], f32) {
        if grit {
            (&self.grit, SK1_RATE)
        } else {
            (&self.raw, self.rate)
        }
    }
}

/// `raw` at `rate`, taken again at the SK-1's rate with no filter first (whatever lies between
/// the new samples is simply skipped: high tones alias), each as a signed 8-bit number.
pub fn crunch(raw: &[f32], rate: f32) -> Vec<f32> {
    let n = (raw.len() as f64 * SK1_RATE as f64 / rate as f64) as usize;
    (0..n)
        .map(|i| {
            let v = raw[((i as f64 * rate as f64 / SK1_RATE as f64) as usize).min(raw.len() - 1)];
            (v.clamp(-1.0, 1.0) * 127.0).round() / 128.0
        })
        .collect()
}

#[derive(Clone, Copy)]
struct Voice {
    note: u8,
    pos: f64,
    gain: f32,
    env: f32,
    held: bool,
}

struct Rec {
    buf: Vec<f32>,
    rate: f32,
    max: usize,
    waiting: bool,
    pre: std::collections::VecDeque<f32>,
    source: Source,
}

struct Engine {
    p: Params,
    sample: Option<Arc<Sample>>,
    voices: Vec<Voice>,
    out_rate: f32,
    rec: Option<Rec>,
    /// the input's level (peak, falling), for the meter
    level: f32,
    /// a recording finished (to keep on disk)
    fresh: bool,
    /// the velocity of the FM-1's next key (it comes before the key)
    next_vel: u8,
}

impl Engine {
    fn note_on(&mut self, note: u8, vel: u8) {
        if self.sample.is_none() {
            return;
        }
        let gain = 0.35 + 0.65 * (vel as f32 / 127.0);
        let v = Voice { note, pos: 0.0, gain, env: 0.0, held: true };
        // the same note again, else a free voice, else the quietest
        let i = self.voices.iter().position(|x| x.note == note).or_else(|| (self.voices.len() < VOICES).then_some(usize::MAX));
        match i {
            Some(usize::MAX) => self.voices.push(v),
            Some(i) => self.voices[i] = v,
            None => {
                let q = (0..self.voices.len()).min_by(|&a, &b| (self.voices[a].env * self.voices[a].gain).total_cmp(&(self.voices[b].env * self.voices[b].gain))).unwrap();
                self.voices[q] = v;
            }
        }
    }

    fn note_off(&mut self, note: u8) {
        for v in self.voices.iter_mut().filter(|v| v.note == note) {
            v.held = false;
        }
    }

    /// One output frame of the voices (mono).
    fn render(&mut self) -> f32 {
        let Some(s) = self.sample.clone() else {
            self.voices.clear();
            return 0.0;
        };
        let (data, rate) = s.data(self.p.grit);
        if data.is_empty() {
            return 0.0;
        }
        let len = data.len() as f64;
        let attack = 1.0 / (0.003 * self.out_rate);
        let release = 1.0 / (self.p.release.max(0.005) * self.out_rate);
        let mut out = 0.0;
        let (grit, looped, reverse) = (self.p.grit, self.p.looped, self.p.reverse);
        let root = 60.0 - 12.0 * self.p.octave as f32;
        let base = rate as f64 / self.out_rate as f64;
        for v in self.voices.iter_mut() {
            v.env = if v.held { (v.env + attack).min(1.0) } else { v.env - release };
            if v.env <= 0.0 && !v.held {
                continue;
            }
            let at = if reverse { len - 1.0 - v.pos } else { v.pos };
            let i = (at.max(0.0) as usize).min(data.len() - 1);
            let x = if grit {
                // each number held until the next: the SK-1's steps
                data[i]
            } else {
                let j = (i + 1).min(data.len() - 1);
                let f = (at - i as f64) as f32;
                data[i] + (data[j] - data[i]) * f
            };
            out += x * v.env * v.gain;
            v.pos += base * 2f64.powf((v.note as f64 - root as f64) / 12.0);
            if v.pos >= len {
                if looped {
                    v.pos %= len;
                } else {
                    v.env = 0.0;
                    v.held = false;
                }
            }
        }
        self.voices.retain(|v| v.held || v.env > 0.0);
        out * self.p.volume
    }

    /// A frame of what is being recorded (from the synth's output or the input).
    fn take(&mut self, x: f32, source: Source) {
        let Some(r) = self.rec.as_mut().filter(|r| r.source == source) else { return };
        self.level = self.level.max(x.abs());
        if r.waiting {
            r.pre.push_back(x);
            if r.pre.len() as f32 > PREROLL_S * r.rate {
                r.pre.pop_front();
            }
            if x.abs() < TRIGGER {
                return;
            }
            r.waiting = false;
            r.buf.extend(r.pre.drain(..));
        }
        r.buf.push(x);
        if r.buf.len() >= r.max {
            self.finish();
        }
    }

    fn finish(&mut self) {
        if let Some(r) = self.rec.take() {
            if !r.buf.is_empty() {
                self.voices.clear();
                self.sample = Some(Arc::new(Sample::new(r.buf, r.rate)));
                self.fresh = true;
            }
        }
    }
}

/// The sampler, shared by the window, the audio output, the MIDI inputs and the input device.
#[derive(Clone)]
pub struct Sampler(Arc<Mutex<Engine>>);

impl Sampler {
    pub fn new(p: Params) -> Sampler {
        Sampler(Arc::new(Mutex::new(Engine { p, sample: None, voices: Vec::new(), out_rate: 44_100.0, rec: None, level: 0.0, fresh: false, next_vel: 100 })))
    }

    fn e(&self) -> std::sync::MutexGuard<'_, Engine> {
        self.0.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn set_params(&self, p: &Params) {
        let mut e = self.e();
        if p.route == Route::Synth {
            e.voices.clear();
        }
        e.p = p.clone();
    }

    pub fn sample(&self) -> Option<Arc<Sample>> {
        self.e().sample.clone()
    }

    pub fn set_sample(&self, s: Option<Sample>) {
        let mut e = self.e();
        e.voices.clear();
        e.sample = s.map(Arc::new);
    }

    pub fn all_off(&self) {
        self.e().voices.iter_mut().for_each(|v| v.held = false);
    }

    /// Where the voices are in the sound (0..1), for the display.
    pub fn playheads(&self) -> Vec<f32> {
        let e = self.e();
        let Some(s) = &e.sample else { return Vec::new() };
        let n = s.data(e.p.grit).0.len().max(1) as f64;
        e.voices.iter().map(|v| {
            let f = (v.pos / n) as f32;
            if e.p.reverse { 1.0 - f } else { f }
        }).collect()
    }

    // -------------------------------------------------------------- the keys

    /// Action `a` on its way to the synth: the sampler plays its notes (as the route says);
    /// true when the synth gets it too.
    pub fn route(&self, smk: bool, a: &Action) -> bool {
        let mut e = self.e();
        let route = e.p.route;
        match *a {
            Action::KeyVel { vel } => {
                e.next_vel = vel;
                true
            }
            Action::Key { id, down } => {
                // FM-1: keys 14..=40 are F3..G5; SMK-37: keys 0..=36 are C3..C6
                let note = if smk { (id <= 36).then_some(48 + id) } else { (14..=40).contains(&id).then_some(39 + id) };
                let Some(note) = note else { return true };
                if route != Route::Synth {
                    let vel = std::mem::replace(&mut e.next_vel, 100);
                    if down { e.note_on(note, vel) } else { e.note_off(note) }
                }
                route != Route::Sampler
            }
            Action::Pressure { .. } => route != Route::Sampler,
            _ => true,
        }
    }

    /// A MIDI message from an input: the sampler plays its notes; true when the synth gets it too.
    pub fn midi(&self, msg: &[u8]) -> bool {
        let mut e = self.e();
        let route = e.p.route;
        let (Some(&st), Some(&note)) = (msg.first(), msg.get(1)) else { return true };
        let vel = msg.get(2).copied().unwrap_or(0);
        match st & 0xf0 {
            0x90 | 0x80 => {
                if route != Route::Synth {
                    if st & 0xf0 == 0x90 && vel > 0 { e.note_on(note, vel) } else { e.note_off(note) }
                }
                route != Route::Sampler
            }
            0xa0 => route != Route::Sampler,
            _ => true,
        }
    }

    // -------------------------------------------------------------- the audio output

    pub fn set_out_rate(&self, r: f32) {
        self.e().out_rate = r;
    }

    #[cfg(test)]
    /// One output frame: the synth's (`l`, `r`) in, recorded if the synth is the source, with the
    /// voices added.
    pub fn mix(&self, l: f32, r: f32) -> (f32, f32) {
        let mut e = self.e();
        if e.rec.is_some() {
            e.take((l + r) * 0.5, Source::Synth);
        }
        if e.voices.is_empty() {
            return (l, r);
        }
        let v = e.render();
        (l + v, r + v)
    }

    /// The same for a whole buffer of interleaved frames: one lock for all of them.
    pub fn mix_frames(&self, out: &mut [f32], channels: usize) {
        let mut e = self.e();
        if e.rec.is_none() && e.voices.is_empty() {
            return;
        }
        for frame in out.chunks_mut(channels) {
            let l = frame[0];
            let r = if channels > 1 { frame[1] } else { l };
            if e.rec.is_some() {
                e.take((l + r) * 0.5, Source::Synth);
            }
            if !e.voices.is_empty() {
                let v = e.render();
                for s in frame.iter_mut() {
                    *s = (*s + v).clamp(-1.0, 1.0);
                }
            }
        }
    }

    // -------------------------------------------------------------- recording

    /// Start recording from the source the params name, `rate` frames a second.
    fn arm(&self, rate: f32) {
        let mut e = self.e();
        let p = e.p.clone();
        e.voices.clear();
        e.level = 0.0;
        e.rec = Some(Rec { buf: Vec::new(), rate, max: (p.length * rate) as usize, waiting: p.wait, pre: Default::default(), source: p.source });
    }

    pub fn recording(&self) -> bool {
        self.e().rec.is_some()
    }

    /// Waiting for sound, recorded seconds, the memory in seconds.
    pub fn rec_state(&self) -> Option<(bool, f32, f32)> {
        let e = self.e();
        e.rec.as_ref().map(|r| (r.waiting, r.buf.len() as f32 / r.rate, r.max as f32 / r.rate))
    }

    /// The level meter (0..1), falling.
    pub fn level(&self) -> f32 {
        let mut e = self.e();
        let l = e.level;
        e.level *= 0.85;
        l
    }

    /// Stop recording: what came so far is the sample.
    pub fn stop(&self) {
        self.e().finish();
    }

    /// Cancel a recording that is still waiting for sound (nothing kept).
    pub fn cancel(&self) {
        self.e().rec = None;
    }

    /// A recording finished since the last ask.
    pub fn take_fresh(&self) -> bool {
        std::mem::replace(&mut self.e().fresh, false)
    }

    /// Record: from the synth through the output, or from the default input device (its stream
    /// is returned, to keep while it records).
    pub fn record(&self) -> Result<Option<cpal::Stream>, String> {
        let (source, out_rate) = { let e = self.e(); (e.p.source, e.out_rate) };
        if source == Source::Synth {
            self.arm(out_rate);
            return Ok(None);
        }
        let host = cpal::default_host();
        let dev = host.default_input_device().ok_or("no input device (microphone / line in)")?;
        let cfg = dev.default_input_config().map_err(|e| e.to_string())?;
        let channels = cfg.channels() as usize;
        let rate = cfg.sample_rate().0 as f32;
        let me = self.clone();
        let err = |e| eprintln!("input: {e}");
        let stream = match cfg.sample_format() {
            cpal::SampleFormat::F32 => dev.build_input_stream(&cfg.config(), move |d: &[f32], _| me.input(d.chunks(channels).map(|f| f.iter().sum::<f32>() / channels as f32)), err, None),
            cpal::SampleFormat::I16 => dev.build_input_stream(&cfg.config(), move |d: &[i16], _| me.input(d.chunks(channels).map(|f| f.iter().map(|&s| s as f32 / 32768.0).sum::<f32>() / channels as f32)), err, None),
            cpal::SampleFormat::U16 => dev.build_input_stream(&cfg.config(), move |d: &[u16], _| me.input(d.chunks(channels).map(|f| f.iter().map(|&s| (s as f32 - 32768.0) / 32768.0).sum::<f32>() / channels as f32)), err, None),
            f => return Err(format!("input format {f:?} not handled")),
        }
        .map_err(|e| e.to_string())?;
        self.arm(rate);
        stream.play().map_err(|e| e.to_string())?;
        Ok(Some(stream))
    }

    fn input(&self, frames: impl Iterator<Item = f32>) {
        let mut e = self.e();
        for x in frames {
            if e.rec.is_none() {
                break;
            }
            e.take(x, Source::Input);
        }
    }
}

// ------------------------------------------------------------------ WAV files

/// A WAV file's sound, mixed to mono: 8 / 16 / 24 / 32-bit PCM or 32 / 64-bit float.
pub fn read_wav(bytes: &[u8]) -> Result<(Vec<f32>, f32), String> {
    let u16le = |b: &[u8], i: usize| u16::from_le_bytes([b[i], b[i + 1]]);
    let u32le = |b: &[u8], i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("not a WAV file".into());
    }
    let (mut fmt, mut data) = (None, None);
    let mut i = 12;
    while i + 8 <= bytes.len() {
        let len = u32le(bytes, i + 4) as usize;
        let body = &bytes[i + 8..(i + 8 + len).min(bytes.len())];
        match &bytes[i..i + 4] {
            b"fmt " if body.len() >= 16 => fmt = Some(body),
            b"data" => data = Some(body),
            _ => {}
        }
        i += 8 + len + (len & 1);
    }
    let (fmt, data) = (fmt.ok_or("no fmt chunk")?, data.ok_or("no data chunk")?);
    let mut tag = u16le(fmt, 0);
    if tag == 0xfffe && fmt.len() >= 26 {
        tag = u16le(fmt, 24);
    }
    let channels = u16le(fmt, 2).max(1) as usize;
    let rate = u32le(fmt, 4) as f32;
    let bits = u16le(fmt, 14) as usize;
    let width = bits / 8;
    if width == 0 || rate <= 0.0 {
        return Err("a WAV file this app cannot read".into());
    }
    let one = |s: &[u8]| -> Option<f32> {
        Some(match (tag, bits) {
            (1, 8) => (s[0] as f32 - 128.0) / 128.0,
            (1, 16) => i16::from_le_bytes([s[0], s[1]]) as f32 / 32768.0,
            (1, 24) => (i32::from_le_bytes([0, s[0], s[1], s[2]]) >> 8) as f32 / 8_388_608.0,
            (1, 32) => i32::from_le_bytes([s[0], s[1], s[2], s[3]]) as f32 / 2_147_483_648.0,
            (3, 32) => f32::from_le_bytes([s[0], s[1], s[2], s[3]]),
            (3, 64) => f64::from_le_bytes(s[..8].try_into().ok()?) as f32,
            _ => return None,
        })
    };
    let mut out = Vec::with_capacity(data.len() / (width * channels));
    for f in data.chunks_exact(width * channels) {
        let mut sum = 0.0;
        for c in f.chunks_exact(width) {
            sum += one(c).ok_or(format!("WAV format {tag}, {bits}-bit: not handled"))?;
        }
        out.push(sum / channels as f32);
    }
    Ok((out, rate))
}

/// The sound as a 16-bit mono WAV file.
pub fn write_wav(path: &Path, s: &[f32], rate: f32) -> std::io::Result<()> {
    let rate = rate.round() as u32;
    let n = (s.len() * 2) as u32;
    let mut b = Vec::with_capacity(44 + s.len() * 2);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + n).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&rate.to_le_bytes());
    b.extend_from_slice(&(rate * 2).to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&n.to_le_bytes());
    for &x in s {
        b.extend_from_slice(&((x.clamp(-1.0, 1.0) * 32767.0).round() as i16).to_le_bytes());
    }
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::write(path, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(hz: f32, rate: f32, secs: f32) -> Vec<f32> {
        (0..(rate * secs) as usize).map(|i| (i as f32 * hz * std::f32::consts::TAU / rate).sin() * 0.5).collect()
    }

    /// Grit is 8-bit at 9.38 kHz: 256 levels at most, the length kept.
    #[test]
    fn grit_is_eight_bits_at_the_sk1_rate() {
        let s = Sample::new(sine(440.0, 48_000.0, 1.0), 48_000.0);
        assert!((s.grit.len() as f32 - SK1_RATE).abs() < 2.0);
        let mut levels: Vec<i32> = s.grit.iter().map(|x| (x * 128.0).round() as i32).collect();
        assert!(s.grit.iter().all(|x| ((x * 128.0).round() - x * 128.0).abs() < 1e-3));
        levels.sort();
        levels.dedup();
        assert!(levels.len() <= 256);
    }

    /// A key an octave up plays twice as fast; the route decides who hears the keys.
    #[test]
    fn keys_play_by_speed_and_route() {
        let sm = Sampler::new(Params { route: Route::Both, grit: false, ..Default::default() });
        sm.set_out_rate(48_000.0);
        sm.set_sample(Some(Sample::new(vec![0.5; 48_000], 48_000.0)));
        // FM-1 key 33 is C5: one octave over C4 (key 21)
        assert!(sm.route(false, &Action::Key { id: 33, down: true }));
        for _ in 0..24_000 {
            sm.mix(0.0, 0.0);
        }
        assert!(sm.playheads().is_empty(), "a 1 s sound an octave up ends after 0.5 s");
        sm.set_params(&Params { route: Route::Sampler, grit: false, ..Default::default() });
        assert!(!sm.route(false, &Action::Key { id: 21, down: true }));
        assert!(sm.route(false, &Action::Key { id: 3, down: true }), "buttons still go to the synth");
        assert!(!sm.midi(&[0x90, 60, 100]));
        assert!(sm.midi(&[0xb0, 1, 64]), "controllers still go to the synth");
        // (past the 3 ms attack)
        for _ in 0..500 {
            sm.mix(0.0, 0.0);
        }
        assert!(sm.mix(0.0, 0.0).0 > 0.1);
        // SMK-37 key 12 is C4 too
        assert!(!sm.route(true, &Action::Key { id: 12, down: true }));
    }

    /// Recording from the synth waits for sound, keeps a little before it, stops at the memory.
    #[test]
    fn records_the_synth_up_to_the_memory() {
        let sm = Sampler::new(Params { source: Source::Synth, length: 0.5, ..Default::default() });
        sm.arm(1000.0);
        for _ in 0..100 {
            sm.mix(0.0, 0.0);
        }
        assert_eq!(sm.rec_state().unwrap().0, true);
        for _ in 0..2000 {
            sm.mix(0.4, 0.4);
        }
        assert!(!sm.recording() && sm.take_fresh());
        let s = sm.sample().unwrap();
        assert_eq!(s.raw.len(), 500);
        assert_eq!(s.raw[0], 0.0, "the pre-roll");
    }

    #[test]
    fn wav_round_trip() {
        let p = std::env::temp_dir().join(format!("fm1-desk-wav-{}.wav", std::process::id()));
        let s = sine(300.0, 22_050.0, 0.2);
        write_wav(&p, &s, 22_050.0).unwrap();
        let (back, rate) = read_wav(&std::fs::read(&p).unwrap()).unwrap();
        assert_eq!((back.len(), rate), (s.len(), 22_050.0));
        assert!(back.iter().zip(&s).all(|(a, b)| (a - b).abs() < 1e-3));
        let _ = std::fs::remove_file(p);
    }
}
