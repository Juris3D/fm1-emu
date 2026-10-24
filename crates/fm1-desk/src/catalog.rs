//! The firmware lists of both boards, the packages on disk, and getting them.
//!
//! Lists: the ones upstream's web pages publish (fetched at start, kept in `catalogs/`), else the
//! last fetched, else the ones built in (web/firmwares.json, web/smk37/firmwares.json at build
//! time). Packages: `firmwares/<board>/<id>.fwsc`, with `index.json` beside them (the launcher's
//! layout, windows/fetch-firmwares.mjs: what one downloaded the other finds). Downloads follow
//! web/fwsc.js `fetchPackage`: the entry's package, else the Firmware Hub's current one, else its
//! manifest, else the .fwsc its installer page (or a script it loads) names; or the package an
//! installer page keeps in a variable, base64.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, Hash)]
pub enum BoardId {
    #[serde(rename = "fm1")]
    Fm1,
    #[serde(rename = "smk37")]
    Smk37,
}

impl BoardId {
    pub const ALL: [BoardId; 2] = [BoardId::Fm1, BoardId::Smk37];
    pub fn dir(self) -> &'static str {
        match self {
            BoardId::Fm1 => "fm1",
            BoardId::Smk37 => "smk37",
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            BoardId::Fm1 => "FM-1",
            BoardId::Smk37 => "SMK-37",
        }
    }
    /// upstream's published list
    fn url(self) -> &'static str {
        match self {
            BoardId::Fm1 => "https://joakimeriksson.github.io/fm1-emu/firmwares.json",
            BoardId::Smk37 => "https://joakimeriksson.github.io/fm1-emu/smk37/firmwares.json",
        }
    }
    fn built_in(self) -> &'static str {
        match self {
            BoardId::Fm1 => include_str!("../../../web/firmwares.json"),
            BoardId::Smk37 => include_str!("../../../web/smk37/firmwares.json"),
        }
    }
    pub fn of(b: fm1_soc::Board) -> BoardId {
        match b {
            fm1_soc::Board::Fm1 => BoardId::Fm1,
            fm1_soc::Board::Smk37 => BoardId::Smk37,
        }
    }
}

/// One entry of a list (the fields this app uses; the rest are kept by serde's default).
#[derive(Clone, Debug, Deserialize, Default)]
pub struct Entry {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub about: String,
    #[serde(default)]
    pub license: String,
    /// M-VAVE's own: brought by the user (`download`: where M-VAVE has it)
    #[serde(default)]
    pub byo: bool,
    #[serde(default)]
    pub download: Option<String>,
    #[serde(default)]
    pub package: Option<String>,
    #[serde(default)]
    pub installer: Option<String>,
    #[serde(default)]
    pub manifest: Option<String>,
    #[serde(default)]
    pub embedded: Option<String>,
    #[serde(default)]
    pub hub: Option<String>,
    #[serde(default)]
    pub repo: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Catalog {
    #[serde(default)]
    pub default: String,
    pub firmwares: Vec<Entry>,
}

/// Where a list came from.
#[derive(Clone, Debug)]
pub enum Source {
    Fetched,
    Cached,
    BuiltIn,
}

/// What is on disk for one firmware (index.json).
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Local {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub about: String,
    pub file: String,
    /// the list's version when this was downloaded (what an update is measured against; the
    /// package may be the Hub's newer one)
    #[serde(default)]
    pub listed: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
struct Index {
    #[serde(default)]
    board: String,
    #[serde(default)]
    default: String,
    #[serde(default)]
    firmwares: BTreeMap<String, Local>,
}

/// A firmware's state on this computer.
#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    Missing,
    Ready,
    /// downloaded, and the list now has another version
    Update(String),
    /// M-VAVE's own: the user's file
    Own,
}

pub struct Store {
    pub root: PathBuf,
}

impl Store {
    /// The app's folder: the executable's, or its parent when that is `bin` (the zip's layout:
    /// Start.cmd, firmwares\, bin\fm1-desk.exe).
    /// A folder it cannot write to (Program Files, a read-only share): the user's own app-data
    /// folder instead (%LOCALAPPDATA%\FM-1 SMK-37 Desk on Windows).
    pub fn here() -> Store {
        let exe = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)).unwrap_or_else(|| PathBuf::from("."));
        let root = if exe.file_name().is_some_and(|n| n.eq_ignore_ascii_case("bin")) { exe.parent().unwrap_or(&exe).to_path_buf() } else { exe };
        if writable(&root) {
            return Store { root };
        }
        let base = std::env::var_os("LOCALAPPDATA")
            .or_else(|| std::env::var_os("XDG_DATA_HOME"))
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("share")))
            .unwrap_or_else(std::env::temp_dir);
        let root = base.join("FM-1 SMK-37 Desk");
        let _ = std::fs::create_dir_all(&root);
        Store { root }
    }

    pub fn fw_dir(&self, b: BoardId) -> PathBuf {
        self.root.join("firmwares").join(b.dir())
    }
    pub fn save_path(&self, b: BoardId, id: &str) -> PathBuf {
        self.root.join("saves").join(b.dir()).join(format!("{}.flash", safe(id)))
    }
    /// The firmware's screen as it last looked (RGB565, 240 x 240): the Library's thumbnail.
    pub fn snap_path(&self, b: BoardId, id: &str) -> PathBuf {
        self.root.join("saves").join(b.dir()).join(format!("{}.lcd", safe(id)))
    }
    fn cache_path(&self, b: BoardId) -> PathBuf {
        self.root.join("catalogs").join(format!("{}.json", b.dir()))
    }

    /// The list: the cached one if there is one and it reads, else the built-in one.
    pub fn catalog(&self, b: BoardId) -> (Catalog, Source) {
        if let Some(c) = std::fs::read_to_string(self.cache_path(b)).ok().and_then(|t| serde_json::from_str(&t).ok()) {
            return (c, Source::Cached);
        }
        (serde_json::from_str(b.built_in()).expect("built-in firmware list"), Source::BuiltIn)
    }

    /// Fetch upstream's list now (and keep it).
    pub fn fetch_catalog(&self, b: BoardId) -> Result<Catalog, String> {
        let text = get(b.url())?.into_string().map_err(|e| e.to_string())?;
        let c: Catalog = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", b.url()))?;
        let p = self.cache_path(b);
        let _ = std::fs::create_dir_all(p.parent().unwrap());
        let _ = std::fs::write(&p, &text);
        Ok(c)
    }

    pub fn locals(&self, b: BoardId) -> BTreeMap<String, Local> {
        self.index(b).firmwares.into_iter().filter(|(_, l)| self.fw_dir(b).join(&l.file).exists()).collect()
    }

    fn index(&self, b: BoardId) -> Index {
        std::fs::read_to_string(self.fw_dir(b).join("index.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    pub fn status(&self, _b: BoardId, e: &Entry, locals: &BTreeMap<String, Local>) -> Status {
        if e.byo {
            return Status::Own;
        }
        match locals.get(&e.id) {
            None => Status::Missing,
            Some(l) => {
                let listed = l.listed.as_deref().unwrap_or(&l.version);
                if !e.version.is_empty() && listed != e.version { Status::Update(e.version.clone()) } else { Status::Ready }
            }
        }
    }

    pub fn package_path(&self, b: BoardId, id: &str) -> Option<PathBuf> {
        let l = self.index(b).firmwares.get(id).cloned()?;
        let p = self.fw_dir(b).join(l.file);
        p.exists().then_some(p)
    }

    /// Download entry `e`'s package for board `b`, check it is one (and for that board), keep it.
    pub fn download(&self, b: BoardId, e: &Entry) -> Result<PathBuf, String> {
        let (bytes, version) = fetch_package(e)?;
        let board = crate::engine::board_of(&bytes).map_err(|err| format!("not a firmware package: {err}"))?;
        if BoardId::of(board) != b {
            return Err(format!("the package is for the {}, not the {}", BoardId::of(board).name(), b.name()));
        }
        let dir = self.fw_dir(b);
        std::fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
        let file = format!("{}.fwsc", safe(&e.id));
        let tmp = dir.join(format!("{file}.part"));
        std::fs::write(&tmp, &bytes).and_then(|_| std::fs::rename(&tmp, dir.join(&file))).map_err(|err| err.to_string())?;
        let mut idx = self.index(b);
        idx.board = b.dir().into();
        idx.firmwares.insert(e.id.clone(), Local {
            name: e.name.clone(),
            version: version.unwrap_or_else(|| e.version.clone()),
            author: e.author.clone(),
            about: e.about.clone(),
            file: file.clone(),
            listed: Some(e.version.clone()),
        });
        let text = serde_json::to_string_pretty(&idx).map_err(|err| err.to_string())?;
        std::fs::write(dir.join("index.json"), text + "\n").map_err(|err| err.to_string())?;
        Ok(dir.join(file))
    }
}

/// Can files be made in `dir`?
fn writable(dir: &Path) -> bool {
    let probe = dir.join(".fm1-desk-write-test");
    let ok = std::fs::write(&probe, b"").is_ok();
    let _ = std::fs::remove_file(&probe);
    ok
}

/// A file name from an id (ids are plain, but never trust a list with paths).
fn safe(id: &str) -> String {
    id.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' { c } else { '_' }).collect()
}

// ------------------------------------------------------------------ HTTP

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(10))
        .timeout_read(std::time::Duration::from_secs(30))
        .user_agent(concat!("fm1-desk/", env!("CARGO_PKG_VERSION")))
        .build()
}

fn get(url: &str) -> Result<ureq::Response, String> {
    agent().get(url).call().map_err(|e| match e {
        ureq::Error::Status(code, _) => format!("{url}: HTTP {code}"),
        other => format!("{url}: {other}"),
    })
}

fn get_bytes(url: &str) -> Result<Vec<u8>, String> {
    let r = get(url)?;
    let mut v = Vec::new();
    std::io::Read::read_to_end(&mut std::io::Read::take(r.into_reader(), 64 << 20), &mut v).map_err(|e| format!("{url}: {e}"))?;
    Ok(v)
}

fn get_text(url: &str) -> Result<String, String> {
    get(url)?.into_string().map_err(|e| format!("{url}: {e}"))
}

fn join(base: &str, rel: &str) -> Result<String, String> {
    url::Url::parse(base).and_then(|b| b.join(rel)).map(|u| u.to_string()).map_err(|e| format!("{base} + {rel}: {e}"))
}

/// The Firmware Hub's current package (and version) for its firmware `hub` (fwsc.js `hubPackage`).
fn hub_package(hub: &str) -> Option<(String, Option<String>)> {
    const HUB: &str = "https://thefirmwarehub.com/";
    let cat: serde_json::Value = serde_json::from_str(&get_text(&format!("{HUB}catalog.json")).ok()?).ok()?;
    let h = cat.get("firmwares")?.as_array()?.iter().find(|h| h.get("id").and_then(|v| v.as_str()) == Some(hub))?;
    let pkg = h.get("pkg")?.as_str()?;
    let version = h.get("version").filter(|v| !v.is_null()).map(|v| v.as_str().map(String::from).unwrap_or_else(|| v.to_string()));
    Some((join(HUB, pkg).ok()?, version))
}

/// fwsc.js `fetchPackage`: the bytes, and the version when the package is the Hub's newer one.
pub fn fetch_package(e: &Entry) -> Result<(Vec<u8>, Option<String>), String> {
    if let (Some(var), Some(inst)) = (&e.embedded, &e.installer) {
        let page = get_text(inst)?;
        let at = page.find(var.as_str()).ok_or(format!("{inst} has no {var}"))?;
        let rest = &page[at + var.len()..];
        let rest = rest.trim_start().strip_prefix('=').ok_or(format!("{inst}: {var} is not assigned"))?.trim_start();
        let q = rest.chars().next().filter(|c| *c == '"' || *c == '\'').ok_or(format!("{inst}: {var} is not a string"))?;
        let body = &rest[1..rest[1..].find(q).map(|i| i + 1).ok_or(format!("{inst}: {var} has no end"))?];
        use base64::Engine as _;
        let bytes = base64::engine::general_purpose::STANDARD.decode(body.trim()).map_err(|err| format!("{inst}: {var}: {err}"))?;
        return Ok((bytes, None));
    }
    let mut last_err = String::from("no package named");
    if let Some(p) = &e.package {
        match get_bytes(p) {
            Ok(b) => return Ok((b, None)),
            Err(err) => last_err = err,
        }
    }
    if let Some(h) = &e.hub {
        if let Some((url, version)) = hub_package(h) {
            if Some(&url) != e.package.as_ref() {
                if let Ok(b) = get_bytes(&url) {
                    return Ok((b, version));
                }
            }
        }
    }
    if let Some(m) = &e.manifest {
        if let Ok(v) = get_text(m).and_then(|t| serde_json::from_str::<serde_json::Value>(&t).map_err(|err| err.to_string())) {
            if let Some(rel) = v.get("package").or_else(|| v.get("file")).and_then(|x| x.as_str()) {
                if let Ok(b) = join(m, rel).and_then(|u| get_bytes(&u)) {
                    return Ok((b, None));
                }
            }
        }
    }
    let Some(inst) = &e.installer else { return Err(last_err) };
    let page = get_text(inst)?;
    let (mut names, mut base) = (fwsc_names(&page), inst.clone());
    if names.is_empty() {
        for src in script_srcs(&page) {
            let Ok(src) = join(inst, &src) else { continue };
            if let Ok(t) = get_text(&src) {
                names = fwsc_names(&t);
                if !names.is_empty() {
                    base = src;
                    break;
                }
            }
        }
    }
    // An installer page may name many firmwares (one author's site lists all of theirs): when
    // the listed package has gone (replaced by a newer version), take that firmware's newer
    // files, same variant, the newest first, not whichever the page names first.
    let tried: Vec<(String, Option<String>)> = match e.package.as_deref().and_then(stem_variant) {
        Some((stem, variant)) => successors(&names, &stem, &variant),
        None => names.into_iter().map(|n| (n, None)).collect(),
    };
    for (n, version) in tried {
        if let Ok(b) = join(&base, &n).and_then(|u| get_bytes(&u)) {
            if crate::engine::board_of(&b).is_ok() {
                return Ok((b, version));
            }
        }
    }
    Err(format!("{last_err}, and {inst} names no package that loads"))
}

/// The model variants a package's name can end in (SMK-37 Elite, MKE-P37, an FM-1 build).
const VARIANTS: [&str; 3] = ["-elite", "-mke", "-fm1"];

/// "…/makkuro37-0.1-mke.fwsc" -> ("makkuro37", "-mke"): the name before its version, and its variant.
fn stem_variant(url: &str) -> Option<(String, String)> {
    let file = url.rsplit('/').next()?.strip_suffix(".fwsc")?;
    let variant = VARIANTS.iter().find(|v| file.ends_with(*v)).map_or("", |v| *v);
    let file = &file[..file.len() - variant.len()];
    // the version starts at the first "-<digit>"
    let at = file.char_indices().find(|&(i, c)| c == '-' && file[i + 1..].starts_with(|d: char| d.is_ascii_digit()))?.0;
    Some((file[..at].to_string(), variant.to_string()))
}

/// The names in `names` of firmware `stem` in `variant`, with their versions, the newest first.
fn successors(names: &[String], stem: &str, variant: &str) -> Vec<(String, Option<String>)> {
    let mut v: Vec<(String, Option<String>)> = names
        .iter()
        .filter_map(|n| {
            let file = n.rsplit('/').next()?.strip_suffix(".fwsc")?;
            let (s, var) = stem_variant(&format!("{file}.fwsc"))?;
            (s == stem && var == variant).then(|| (n.clone(), Some(file[s.len() + 1..file.len() - var.len()].to_string())))
        })
        .collect();
    v.dedup();
    let key = |ver: &Option<String>| -> Vec<u64> { ver.as_deref().unwrap_or("").split(|c: char| !c.is_ascii_digit()).filter_map(|x| x.parse().ok()).collect() };
    v.sort_by(|a, b| key(&b.1).cmp(&key(&a.1)));
    v
}

/// The `[\w./-]+\.fwsc` names in a page, M-VAVE's own FM-1.fwsc left out (fwsc.js's `find`).
fn fwsc_names(text: &str) -> Vec<String> {
    let ok = |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '/' || c == '-';
    let mut out = Vec::new();
    let b = text.as_bytes();
    let mut i = 0;
    while let Some(k) = text[i..].find(".fwsc") {
        let end = i + k + 5;
        let mut s = i + k;
        while s > 0 && ok(b[s - 1] as char) {
            s -= 1;
        }
        // (the regex is greedy: a name continues while the characters do)
        let mut e2 = end;
        while e2 < b.len() && ok(b[e2] as char) {
            e2 += 1;
        }
        let name = &text[s..e2];
        if name.ends_with(".fwsc") && s < i + k {
            let file = name.rsplit('/').next().unwrap_or(name);
            if !file.eq_ignore_ascii_case("FM-1.fwsc") {
                out.push(name.to_string());
            }
        }
        i = end;
    }
    out
}

/// `src="….js"` / `….mjs` in a page.
fn script_srcs(page: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(k) = page[i..].find("src=\"") {
        let s = i + k + 5;
        let Some(e) = page[s..].find('"') else { break };
        let v = &page[s..s + e];
        if v.ends_with(".js") || v.ends_with(".mjs") {
            out.push(v.to_string());
        }
        i = s + e;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_lists_read() {
        for b in BoardId::ALL {
            let c: Catalog = serde_json::from_str(b.built_in()).unwrap();
            assert!(c.firmwares.len() > 5, "{}", b.name());
            assert!(c.firmwares.iter().any(|e| e.id == c.default), "{}", b.name());
        }
    }

    #[test]
    fn names_as_the_page_finds_them() {
        let page = r#"<a href="firmware/x0x-1.1.0.fwsc">x</a> const P = './fw/orbit-0.7.fwsc'; FM-1.fwsc "../FM-1.fwsc" "#;
        assert_eq!(fwsc_names(page), vec!["firmware/x0x-1.1.0.fwsc", "./fw/orbit-0.7.fwsc"]);
        assert_eq!(script_srcs(r#"<script src="app.js"></script><img src="a.png"><script type=module src="m/x.mjs">"#), vec!["app.js", "m/x.mjs"]);
        assert_eq!(join("https://a.github.io/x/install/", "../firmware/p.fwsc").unwrap(), "https://a.github.io/x/firmware/p.fwsc");
    }

    /// A tiny web server: path -> (status, body), for as many requests as come.
    fn serve(routes: Vec<(&'static str, u16, Vec<u8>)>) -> String {
        use std::io::{Read, Write};
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", l.local_addr().unwrap());
        std::thread::spawn(move || {
            for c in l.incoming() {
                let Ok(mut c) = c else { continue };
                let mut req = [0u8; 4096];
                let n = c.read(&mut req).unwrap_or(0);
                let line = String::from_utf8_lossy(&req[..n]).lines().next().unwrap_or("").to_string();
                let path = line.split(' ').nth(1).unwrap_or("/").to_string();
                let (code, body) = routes.iter().find(|r| r.0 == path).map(|r| (r.1, r.2.clone())).unwrap_or((404, b"nope".to_vec()));
                let _ = write!(c, "HTTP/1.1 {code} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                let _ = c.write_all(&body);
            }
        });
        base
    }

    /// fetchPackage's ways, against a local server: a package that has gone, found through the
    /// installer page's script; one an installer page keeps in a variable, base64; and the store
    /// keeps a package only for its own board.
    #[test]
    fn packages_are_found_as_the_page_finds_them() {
        use base64::Engine as _;
        let pkg = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../fm1-sid/web/sidplayer.fwsc")).unwrap();
        let b64 = base64::engine::general_purpose::STANDARD.encode(&pkg);
        let base = serve(vec![
            ("/install/", 200, br#"<html><script src="app.js"></script></html>"#.to_vec()),
            ("/install/app.js", 200, br#"const P = "../firmware/sid-0.1.fwsc"; // not FM-1.fwsc"#.to_vec()),
            ("/firmware/sid-0.1.fwsc", 200, pkg.clone()),
            ("/emb/", 200, format!("<script>const DEFAULT_FIRMWARE = '{b64}';</script>").into_bytes()),
        ]);
        let gone = Entry { id: "sid".into(), name: "SID".into(), version: "0.1".into(),
            package: Some(format!("{base}/gone.fwsc")), installer: Some(format!("{base}/install/")), ..Default::default() };
        assert_eq!(fetch_package(&gone).unwrap().0, pkg);
        let emb = Entry { id: "e".into(), name: "E".into(), embedded: Some("DEFAULT_FIRMWARE".into()), installer: Some(format!("{base}/emb/")), ..Default::default() };
        assert_eq!(fetch_package(&emb).unwrap().0, pkg);
        let none = Entry { id: "n".into(), name: "N".into(), package: Some(format!("{base}/gone.fwsc")), ..Default::default() };
        assert!(fetch_package(&none).unwrap_err().contains("404"));

        let root = std::env::temp_dir().join(format!("fm1-desk-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let store = Store { root: root.clone() };
        assert!(store.download(BoardId::Smk37, &gone).unwrap_err().contains("for the FM-1"));
        let p = store.download(BoardId::Fm1, &gone).unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), pkg);
        let locals = store.locals(BoardId::Fm1);
        assert_eq!(store.status(BoardId::Fm1, &gone, &locals), Status::Ready);
        assert_eq!(store.package_path(BoardId::Fm1, "sid"), Some(p));
        let newer = Entry { version: "0.2".into(), ..gone };
        assert_eq!(store.status(BoardId::Fm1, &newer, &locals), Status::Update("0.2".into()));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// genkigenki's site names all his firmwares and drops old versions: a gone package is
    /// followed to its own newest file, in its own variant.
    #[test]
    fn a_gone_package_is_followed_to_its_newer_version() {
        let names: Vec<String> = ["firmware/amb-1.1-elite.fwsc", "firmware/makkuro37-0.3-elite.fwsc", "firmware/makkuro37-0.3-mke.fwsc",
            "firmware/makkuro37-0.3.fwsc", "firmware/makkuro37-0.10.fwsc", "firmware/makkuro37x-0.9.fwsc", "firmware/vector37-0.3.2-fm1.fwsc"]
            .iter().map(|s| s.to_string()).collect();
        let (stem, var) = stem_variant("https://x/firmware/makkuro37-0.1.fwsc").unwrap();
        assert_eq!((stem.as_str(), var.as_str()), ("makkuro37", ""));
        let got = successors(&names, &stem, &var);
        assert_eq!(got, vec![("firmware/makkuro37-0.10.fwsc".to_string(), Some("0.10".to_string())), ("firmware/makkuro37-0.3.fwsc".to_string(), Some("0.3".to_string()))]);
        let (stem, var) = stem_variant("https://x/firmware/makkuro37-0.1-mke.fwsc").unwrap();
        assert_eq!(successors(&names, &stem, &var)[0].0, "firmware/makkuro37-0.3-mke.fwsc");
        assert_eq!(stem_variant("https://x/f/choralroot-0.24-beta3-elite.fwsc").unwrap(), ("choralroot".into(), "-elite".into()));
    }

    #[test]
    fn status_measures_against_the_listed_version() {
        let s = Store { root: std::env::temp_dir().join("fm1-desk-test-none") };
        let e = Entry { id: "x".into(), name: "X".into(), version: "1.1".into(), ..Default::default() };
        let mut l = BTreeMap::new();
        assert_eq!(s.status(BoardId::Fm1, &e, &l), Status::Missing);
        l.insert("x".into(), Local { version: "1.2".into(), listed: Some("1.1".into()), ..Default::default() });
        assert_eq!(s.status(BoardId::Fm1, &e, &l), Status::Ready); // (the Hub's newer one)
        l.insert("x".into(), Local { version: "1.0".into(), listed: None, ..Default::default() });
        assert_eq!(s.status(BoardId::Fm1, &e, &l), Status::Update("1.1".into()));
    }
}
