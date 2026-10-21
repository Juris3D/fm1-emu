// Download the community firmwares for BOTH boards into firmwares/<board>/<id>.fwsc, with an
// index.json per board that the launcher reads. Same catalogs and same download code as the
// web pages (web/firmwares.json, web/smk37/firmwares.json, web/fwsc.js), so an upstream update
// of those files is picked up here without changes.
//
//   node windows/fetch-firmwares.mjs                 every firmware, both boards
//   node windows/fetch-firmwares.mjs --board smk37   one board (fm1 | smk37)
//   node windows/fetch-firmwares.mjs felucca snoop37 only these ids (on whichever board has them)
//   node windows/fetch-firmwares.mjs --missing       only those not downloaded yet
//
// Windows-safe paths (fileURLToPath), unlike tools/fetch_firmwares.mjs, which uses URL.pathname.
// M-VAVE's own firmwares ("byo") are never downloaded: open your own FM-1.fwsc / SMK-37 Pro_016.fwsc
// from the launcher.
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));
const { appFromPackage, fetchPackage } = await import(pathToFileURL(join(root, "web", "fwsc.js")).href);

export const BOARDS = {
  fm1: { name: "FM-1", catalog: join(root, "web", "firmwares.json") },
  smk37: { name: "SMK-37", catalog: join(root, "web", "smk37", "firmwares.json") },
};

function parseArgs(argv) {
  const o = { boards: Object.keys(BOARDS), ids: new Set(), missing: false };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === "--board") {
      const b = argv[++i];
      if (!BOARDS[b]) throw new Error(`--board ${b}: fm1 or smk37`);
      o.boards = [b];
    } else if (a === "--missing") o.missing = true;
    else if (a.startsWith("--")) throw new Error(`unknown option ${a}`);
    else o.ids.add(a);
  }
  return o;
}

export async function fetchAll(argv, { fetchFn = fetch, log = console.log } = {}) {
  const o = parseArgs(argv);
  let ok = 0, failed = 0;
  for (const board of o.boards) {
    const cat = JSON.parse(readFileSync(BOARDS[board].catalog, "utf8"));
    const dir = join(root, "firmwares", board);
    mkdirSync(dir, { recursive: true });
    const indexPath = join(dir, "index.json");
    const index = existsSync(indexPath) ? JSON.parse(readFileSync(indexPath, "utf8")) : { firmwares: {} };
    index.board = board;
    index.default = cat.default;
    log(`== ${BOARDS[board].name} (${cat.firmwares.length} in the catalog)`);
    for (const f of cat.firmwares) {
      if (f.byo) continue;
      if (o.ids.size && !o.ids.has(f.id)) continue;
      const file = join(dir, `${f.id}.fwsc`);
      if (o.missing && existsSync(file)) continue;
      try {
        const { bytes } = await fetchPackage(f, fetchFn);
        appFromPackage(bytes); // throws on a package that is not one
        writeFileSync(file, bytes);
        index.firmwares[f.id] = { name: f.name, version: f.version, author: f.author || "",
                                  about: f.about || "", file: `${f.id}.fwsc` };
        log(`  ${f.id.padEnd(14)} ${f.name} ${f.version}`);
        ok++;
      } catch (e) {
        log(`  ${f.id.padEnd(14)} FAILED: ${e.message}`);
        failed++;
      }
    }
    // the catalog's order, only packages that are on disk (one deleted by hand drops out)
    const ordered = {};
    for (const f of cat.firmwares) {
      const e = index.firmwares[f.id];
      if (e && existsSync(join(dir, e.file))) ordered[f.id] = e;
    }
    index.firmwares = ordered;
    writeFileSync(indexPath, JSON.stringify(index, null, 1) + "\n");
  }
  log(`${ok} downloaded, ${failed} failed`);
  return { ok, failed };
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  try {
    const { failed, ok } = await fetchAll(process.argv.slice(2));
    process.exit(failed && !ok ? 1 : 0);
  } catch (e) {
    console.error(e.message);
    process.exit(2);
  }
}
