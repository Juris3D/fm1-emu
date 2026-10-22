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
    pub fn here() -> Store {
        let exe = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)).unwrap_or_else(|| PathBuf::from("."));
        let root = if exe.file_name().is_some_and(|n| n.eq_ignore_ascii_case("bin")) { exe.parent().unwrap_or(&exe).to_path_buf() } else { exe };
        Store { root }
    }

    pub fn fw_dir(&self, b: BoardId) -> PathBuf {
        self.root.join("firmwares").join(b.dir())
    }
    pub fn save_path(&self, b: BoardId, id: &str) -> PathBuf {
        self.root.join("saves").join(b.dir()).join(format!("{}.flash", safe(id)))
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
    for n in names {
        if let Ok(b) = join(&base, &n).and_then(|u| get_bytes(&u)) {
            return Ok((b, None));
        }
    }
    Err(format!("{last_err}, and {inst} names no package that loads"))
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
