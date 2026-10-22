//! What this binary was built from (build.rs), and whether the checkout has moved on since: a
//! stale binary's halts and wrong results look like firmware bugs (fm1-live includes this file).
use std::process::Command;

/// `fm1-emu 0.2.0 (abc1234, 2026-10-21)`
pub fn version(name: &str) -> String {
    format!("{name} {} ({}, {})", env!("CARGO_PKG_VERSION"), env!("FM1_GIT"), env!("FM1_GIT_DATE"))
}

/// When the checkout this binary was built in is at another commit now: say so, and to rebuild.
pub fn stale() -> Option<String> {
    let built = env!("FM1_GIT").trim_end_matches("-dirty");
    let out = Command::new("git").args(["-C", env!("CARGO_MANIFEST_DIR"), "rev-parse", "--short=9", "HEAD"]).output().ok()?;
    let head = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (out.status.success() && built != "unknown" && head != built)
        .then(|| format!("this binary is from {built}, the checkout is at {head} now: rebuild it before suspecting the firmware"))
}

/// The lines after a halt: which binary halted, and whether it is stale.
pub fn halt_note(name: &str) -> String {
    let mut s = format!("(this binary: {})", version(name));
    if let Some(w) = stale() {
        s += &format!("\nwarning: {w}");
    }
    s
}
