//! Shared benchmark input script, scheduled on the emulated clock.
use crate::{Action, Soc};

pub fn queue(soc: &mut Soc, keys: &str) {
    for k in keys.split(',') {
        if let Some((t, rest)) = k.split_once(':') {
            // `ms:mHEX` sends MIDI bytes into the MIDI IN jack (`ms:m903c64` = note on C4)
            if let Some(hex) = rest.trim().strip_prefix('m') {
                if let Ok(t) = t.trim().parse::<u64>() {
                    let at = soc.periph.cycle_of_ms(t);
                    for i in (0..hex.len() & !1).step_by(2) {
                        if let Ok(b) = u8::from_str_radix(&hex[i..i + 2], 16) {
                            soc.periph.script.push_back((at, Action::Midi { b }));
                        }
                    }
                }
                continue;
            }
            // `ms:eN:clicks` turns matrix encoder N
            if let Some((e, c)) = rest.strip_prefix('e').and_then(|r| r.split_once(':')) {
                if let (Ok(t), Ok(enc), Ok(clicks)) = (t.trim().parse::<u64>(), e.parse::<u8>(), c.parse::<i32>()) {
                    let at = soc.periph.cycle_of_ms(t);
                    soc.periph.script.push_back((at, Action::Enc { enc, clicks }));
                }
                continue;
            }
            // `ms:ID@VEL` presses at velocity VEL (1..127; FM-1: how hard it lands on its pill),
            // `ms:ID~VEL` presses a held key to VEL's force (aftertouch)
            let (rest, vel, pressure) = match rest.trim().split_once(['@', '~']) {
                Some((i, v)) => (i, v.parse::<u8>().ok(), rest.contains('~')),
                None => (rest, None, false),
            };
            let id = rest;
            let (id, down) = match id.trim().strip_suffix('^') { Some(i) => (i, false), None => (id.trim(), true) };
            // a number, or a name on this board's panel (FM-1: `PLAY`, `C4`; SMK-37: `seqplay`, `k12`)
            let id = id.parse::<u8>().ok().or_else(|| match soc.board {
                crate::Board::Smk37 => crate::smk::key_id(id),
                crate::Board::Fm1 => crate::periph::key_id(id),
            });
            if let (Ok(t), Some(id)) = (t.trim().parse::<u64>(), id) {
                let at = soc.periph.cycle_of_ms(t);
                match (vel, pressure) {
                    (Some(vel), true) => soc.periph.script.push_back((at, Action::Pressure { id, vel })),
                    (Some(vel), false) => {
                        soc.periph.script.push_back((at, Action::KeyVel { vel }));
                        soc.periph.script.push_back((at, Action::Key { id, down }));
                    }
                    _ => soc.periph.script.push_back((at, Action::Key { id, down })),
                }
            }
        }
    }
    soc.periph.script.make_contiguous().sort_by_key(|(c, _)| *c);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn schedules_keys_encoders_and_midi_in_stable_time_order() {
        let mut soc = Soc::new(1 << 20, vec![]);
        queue(&mut soc, "100:21^,50:e2:-3,50:m903c64,0:21");
        let actions: Vec<_> = soc.periph.script.iter().map(|(_, a)| a.clone()).collect();
        assert_eq!(actions, vec![Action::Key { id: 21, down: true },
            Action::Enc { enc: 2, clicks: -3 }, Action::Midi { b: 0x90 },
            Action::Midi { b: 0x3c }, Action::Midi { b: 0x64 },
            Action::Key { id: 21, down: false }]);
        assert_eq!(soc.periph.script.back().unwrap().0, soc.periph.cycle_of_ms(100));
    }

    #[test]
    fn velocity_and_pressure() {
        let mut soc = Soc::new(1 << 20, vec![]);
        queue(&mut soc, "0:21@40,10:21~120,20:21^");
        let actions: Vec<_> = soc.periph.script.iter().map(|(_, a)| a.clone()).collect();
        assert_eq!(actions, vec![Action::KeyVel { vel: 40 }, Action::Key { id: 21, down: true },
            Action::Pressure { id: 21, vel: 120 }, Action::Key { id: 21, down: false }]);
    }
}
