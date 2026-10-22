//! P33, the power domain's analog control registers (supplies, watchdog, reset source), reached
//! through a byte-serial port: P33_CON at 0x13E08, P33_DAT at 0x13E0C (SDK p33.h).
//!
//! A transfer writes a byte to DAT, sets CON bit 4 to start it, waits while CON bit 1 (busy)
//! is set and reads the byte that came back from DAT. Each access is three transfers with CON
//! bit 0 (chip select) held: a command (bit 7 read; bits 5..6 the write's operation: 0 write,
//! 1 or, 2 and, 3 xor; bits 0..1 the address's top bits), the address's low byte, and the data
//! (a read's dummy byte, its value coming back). CON bit 8 selects the RTC domain's registers.
//!
//! Modelled as plain registers, all 0 at power-on, so a firmware reads back what it set. Before
//! this every read returned 0, and Melodee, which checks its supply setup that way, stopped at
//! boot. Nothing here acts on the values (no watchdog, no reset through PR_PWR).

#[derive(Clone)]
pub struct P33 {
    con: u32,
    dat: u32,
    /// transfers since chip select, and what the first two said
    step: u8,
    cmd: u8,
    addr: u16,
    /// main domain at 0..0x400, RTC domain (CON bit 8) at 0x400..0x800
    regs: Box<[u8; 0x800]>,
}

impl Default for P33 {
    fn default() -> Self {
        Self { con: 0, dat: 0, step: 0, cmd: 0, addr: 0, regs: Box::new([0; 0x800]) }
    }
}

impl P33 {
    pub const CON: u32 = 0x13e08;
    pub const DAT: u32 = 0x13e0c;

    /// A register's value as the firmware last left it (`rtc`: the RTC domain's bank).
    pub fn reg(&self, addr: u16, rtc: bool) -> u8 {
        self.regs[Self::index(addr, rtc)]
    }

    fn index(addr: u16, rtc: bool) -> usize {
        (addr as usize & 0x3ff) | if rtc { 0x400 } else { 0 }
    }

    pub fn read(&self, addr: u32) -> u32 {
        match addr & !3 {
            // transfers finish at once: never busy, never still starting
            Self::CON => self.con & !0x12,
            _ => self.dat,
        }
    }

    pub fn write(&mut self, addr: u32, value: u32) {
        if addr & !3 == Self::DAT {
            self.dat = value;
            return;
        }
        if value & 1 == 0 {
            self.step = 0; // deselected: the next transfer is a new command
        }
        self.con = value;
        if value & 0x10 != 0 && value & 1 != 0 {
            self.dat = self.transfer(self.dat as u8) as u32;
        }
    }

    fn transfer(&mut self, b: u8) -> u8 {
        let step = self.step;
        self.step = self.step.saturating_add(1);
        match step {
            0 => {
                self.cmd = b;
                self.addr = ((b & 3) as u16) << 8;
                0
            }
            1 => {
                self.addr |= b as u16;
                0
            }
            2 => {
                let i = Self::index(self.addr, self.con & 0x100 != 0);
                if self.cmd & 0x80 != 0 {
                    return self.regs[i];
                }
                let r = &mut self.regs[i];
                *r = match (self.cmd >> 5) & 3 {
                    0 => b,
                    1 => *r | b,
                    2 => *r & b,
                    _ => *r ^ b,
                };
                0
            }
            _ => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One access as the SDK's p33 driver makes it.
    fn access(p: &mut P33, cmd: u8, addr: u8, data: u8, rtc: bool) -> u8 {
        let cs = 1 | if rtc { 0x100 } else { 0 };
        p.write(P33::CON, cs);
        let mut last = 0;
        for b in [cmd, addr, data] {
            p.write(P33::DAT, b as u32);
            p.write(P33::CON, p.read(P33::CON) | 0x10);
            assert_eq!(p.read(P33::CON) & 2, 0, "never busy");
            last = p.read(P33::DAT) as u8;
        }
        p.write(P33::CON, p.read(P33::CON) & !0x101);
        last
    }

    #[test]
    fn reads_back_what_was_written_and_combined() {
        let mut p = P33::default();
        assert_eq!(access(&mut p, 0x80, 0x05, 0x5e, false), 0, "0 at power-on");
        access(&mut p, 0x00, 0x05, 0x40, false); // write
        access(&mut p, 0x20, 0x05, 0x04, false); // or
        assert_eq!(access(&mut p, 0x80, 0x05, 0x5e, false), 0x44);
        access(&mut p, 0x40, 0x05, 0xfb, false); // and
        access(&mut p, 0x60, 0x05, 0x30, false); // xor
        assert_eq!(access(&mut p, 0x80, 0x05, 0x5e, false), 0x70);
        // the address's top bits come from the command; the RTC domain is its own bank
        access(&mut p, 0x01, 0x05, 0x11, false);
        access(&mut p, 0x00, 0x05, 0x22, true);
        assert_eq!((p.reg(0x105, false), p.reg(0x05, false), p.reg(0x05, true)), (0x11, 0x70, 0x22));
    }
}
