// FM1_GIT: the commit this binary is built from (`abc1234`, `abc1234-dirty` with uncommitted
// changes, `unknown` outside a checkout) and FM1_GIT_DATE its date, for --version and the halt
// report (src/buildinfo.rs). fm1-live includes this file as its own build script.
use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn main() {
    let id = match git(&["rev-parse", "--short=9", "HEAD"]) {
        Some(c) if git(&["status", "--porcelain", "--untracked-files=no"]).is_some_and(|s| !s.is_empty()) => format!("{c}-dirty"),
        Some(c) => c,
        None => "unknown".into(),
    };
    println!("cargo::rustc-env=FM1_GIT={id}");
    println!("cargo::rustc-env=FM1_GIT_DATE={}", git(&["log", "-1", "--format=%cs"]).unwrap_or_default());
    // again when HEAD moves, and when a source changes (the -dirty mark)
    let mut watch = vec!["HEAD".to_string(), "index".into(), "packed-refs".into()];
    watch.extend(git(&["symbolic-ref", "-q", "HEAD"]));
    for p in watch {
        if let Some(f) = git(&["rev-parse", "--git-path", &p]) {
            println!("cargo::rerun-if-changed={f}");
        }
    }
    println!("cargo::rerun-if-changed=../../crates");
}
