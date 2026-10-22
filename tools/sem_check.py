#!/usr/bin/env python3
"""Check what the emulator computes against what the vendor disassembler says an instruction does.

    tools/sem_check.py LISTING.txt [LISTING2.txt ...] [--emu target/release/fm1-emu] [--n 6] [--seed 1]

For every distinct instruction in the objdump listings whose text this script can evaluate
(register arithmetic: ALU ops, shifts, rotates, saturating ops, 64-bit pair ops, extracts,
byte / bit operations; not memory, branches or the carry rotates), runs it from random
registers with `fm1-emu --sem-run` and compares the registers afterwards with the text's
meaning. Prints the disagreements grouped by the text's shape, with examples, and a summary.

The listings are the vendor objdump's, of code the vendor toolchain made (probes of the
compiler's builtins and the assembler, the M-VAVE engine library, corpus firmwares).
"""
import collections, random, re, subprocess, sys, tempfile

args = sys.argv[1:]
def opt(name, default):
    if name in args:
        i = args.index(name)
        v = args[i + 1]
        del args[i:i + 2]
        return v
    return default
emu = opt("--emu", "target/release/fm1-emu")
N = int(opt("--n", "6"))
rng = random.Random(int(opt("--seed", "1")))
listings = [a for a in args if not a.startswith("--")]

M = 0xFFFFFFFF
M64 = (1 << 64) - 1
def s32(x): x &= M; return x - (1 << 32) if x & 0x80000000 else x
def s64(x): x &= M64; return x - (1 << 64) if x >> 63 else x
def clamp(v, lo, hi): return max(lo, min(hi, v))
def ssat(v): return clamp(v, -(1 << 31), (1 << 31) - 1) & M
def num(t): return int(t, 0)
def rot(x, n): n %= 32; return ((x >> n) | (x << (32 - n))) & M if n else x
def brev(x): return int(f"{x & M:032b}"[::-1], 2)
def clz(x): return 32 - (x & M).bit_length()
def cls(x):
    x = s32(x)
    return clz(~x & M) - 1 if x < 0 else clz(x) - 1

R = r"r(\d+)"
IMM = r"(-?(?:0x[0-9A-Fa-f]+|\d+))"
SRC = rf"(?:{R}|{IMM})"
OPS = {"+": lambda a, b: a + b, "-": lambda a, b: a - b, "&": lambda a, b: a & b, "|": lambda a, b: a | b,
       "^": lambda a, b: a ^ b, "*": lambda a, b: a * b}

def shift(op, x, n):
    """32-bit shifts as the instruction text names them; None for amounts the hardware is not
    known to define (by-register amounts are kept in 0..31 by the generator)"""
    x &= M
    if op == "<<": return (x << n) & M if n < 32 else 0
    if op == ">>": return x >> n if n < 32 else 0
    if op == ">>>": return (s32(x) >> min(n, 31)) & M
    if op == "<<<": return ssat(s32(x) << n) if x else 0      # qasl: saturating
    if op == "<>": return rot(x, n)
    return None

class Sem:
    """parse once: which registers are inputs (and must hold shift amounts), and the evaluator"""
    def __init__(self, text):
        self.amount_regs = []   # registers whose value is a shift amount: kept small
        self.fn = None
        t = re.sub(r"<[^>]*>", "", text).replace("#", "").strip()
        self.t = t
        for pat, mk in RULES:
            m = re.fullmatch(pat, t)
            if m:
                self.fn = mk(self, m)
                return

    def amount(self, reg):
        self.amount_regs.append(int(reg))
        return int(reg)

def pair(r, h, l): return (r[int(h)] << 32) | r[int(l)]
def set_pair(r, h, l, v): r[int(h)] = (v >> 32) & M; r[int(l)] = v & M

def alu3(S, m):
    d, a, op, rb, imm = int(m[1]), int(m[2]), m[3], m[4], m[5]
    def f(r):
        b = r[int(rb)] if rb else num(imm) & M
        r[d] = OPS[op](r[a], b) & M
    return f
def alu2(S, m):
    d, op, rb, imm = int(m[1]), m[2], m[3], m[4]
    def f(r):
        b = r[int(rb)] if rb else num(imm) & M
        r[d] = OPS[op](r[d], b) & M
    return f
def sh3(S, m):
    d, a, op, rb, imm = int(m[1]), int(m[2]), m[3], m[4], m[5]
    if rb: S.amount(rb)
    def f(r):
        n = r[int(rb)] & 0xff if rb else num(imm)
        v = shift(op, r[a], n)
        if v is None: raise KeyError
        r[d] = v
    return f
def sh2(S, m):
    d, op, rb, imm = int(m[1]), m[2], m[3], m[4]
    if rb: S.amount(rb)
    def f(r):
        n = r[int(rb)] & 0xff if rb else num(imm)
        r[d] = shift(op, r[d], n)
    return f
def pair_sh(S, m):
    h, l, op, rb, imm = m[1], m[2], m[3], m[4], m[5]
    if rb: S.amount(rb)
    def f(r):
        n = (r[int(rb)] if rb else num(imm)) & 63
        x = pair(r, h, l)
        v = {"<<": x << n, ">>": x >> n, ">>>": s64(x) >> n, "<>": (x >> n) | (x << (64 - n)) if n else x}[op]
        set_pair(r, h, l, v & M64)
    return f
def pair_ext(S, m):
    d, h, l, rb, imm, su = int(m[1]), m[2], m[3], m[4], m[5], m[6]
    if rb: S.amount(rb)
    def f(r):
        n = (r[int(rb)] if rb else num(imm)) & 63
        x = pair(r, h, l)
        r[d] = clamp(s64(x) >> n, -(1 << 31), (1 << 31) - 1) & M if su == "s" else min(x >> n, M)
    return f
def pair_mul(S, m):
    h, l, op, a, b, su = m[1], m[2], m[3], int(m[4]), int(m[5]), m[6]
    def f(r):
        x, y = (s32(r[a]), s32(r[b])) if su == "s" else (r[a], r[b])
        p = x * y
        acc = pair(r, h, l)
        set_pair(r, h, l, (p if op == "=" else acc + p if op == "+=" else acc - p) & M64)
    return f
def ssat3(S, m):
    d, a, op, b = int(m[1]), int(m[2]), m[3], int(m[4])
    def f(r): r[d] = ssat(s32(r[a]) + s32(r[b]) if op == "+" else s32(r[a]) - s32(r[b]))
    return f
def un(fn):
    def mk(S, m):
        d, s = int(m[1]), int(m[2])
        def f(r): r[d] = fn(r[s]) & M
        return f
    return mk
def sat16(S, m):
    d, s, su = int(m[1]), int(m[2]), m[3]
    def f(r): r[d] = (clamp(s32(r[s]), -32768, 32767) if su == "s" else clamp(s32(r[s]), 0, 65535)) & M
    return f
def minmax(S, m):
    d, op, a, b = int(m[1]), m[2], int(m[3]), int(m[4])
    def f(r):
        x, y = (s32(r[a]), s32(r[b])) if op[0] == "s" else (r[a], r[b])
        r[d] = (max(x, y) if op[1:] == "max" else min(x, y)) & M
    return f
def brevadd(S, m):
    d, s = int(m[1]), int(m[2])
    def f(r): r[d] = brev((brev(r[d]) + brev(r[s])) & M)
    return f
def byteset(S, m):
    d, k, imm = int(m[1]), int(m[2]), num(m[3]) & 0xff
    def f(r): r[d] = (r[d] & ~(0xff << 8 * k) | imm << 8 * k) & M
    return f
def bitop(S, m):
    d, a, op, b = int(m[1]), int(m[2]), m[3], int(m[4])
    S.amount(m[4])
    def f(r):
        bit = 1 << (r[b] & 31)
        r[d] = {"|": r[a] | bit, "^": r[a] ^ bit, "&": r[a] & bit, "& ~": r[a] & ~bit & M}[op]
    return f
def movi(S, m):
    d, imm = int(m[1]), num(m[2])
    def f(r): r[d] = imm & M
    return f
def rsub(S, m):
    d, imm, a = int(m[1]), num(m[2]), int(m[3])
    def f(r): r[d] = (imm - r[a]) & M
    return f
def ext(S, m):
    d, s, part, su = int(m[1]), int(m[2]), m[3], m[4]
    def f(r):
        w = 8 if part == "b0" else 16
        v = r[s] & ((1 << w) - 1)
        if su == "s" and v >> (w - 1): v -= 1 << w
        r[d] = v & M
    return f
def extra(S, m):
    d, su, s, p, l = int(m[1]), m[2], int(m[3]), int(m[4]), int(m[5])
    def f(r):
        v = (r[s] >> p) & ((1 << l) - 1)
        if su == "s" and v >> (l - 1): v -= 1 << l
        r[d] = v & M
    return f

SH = r"(<<<|<<|>>>|>>|<>)"
RULES = [
    (rf"{R} = {R} \+ {R} \(ssat\)".replace(r"\+", r"([+-])"), ssat3),
    (rf"{R} = {R} ([+\-&|^*]) {SRC}", alu3),
    (rf"{R} ([+\-&|^*])= {SRC}", alu2),
    (rf"{R} = {R} {SH} {SRC}", sh3),
    (rf"{R} {SH}= {SRC}", sh2),
    (rf"{R}_{R} {SH}= {SRC}", pair_sh),
    (rf"{R} = {R}_{R} >> {SRC} \(([su])\)", pair_ext),
    (rf"{R}_{R} (=|\+=|-=) {R} \* {R} \(([su])\)", pair_mul),
    (rf"{R} = sat16\({R}\) \(([su])\)", sat16),
    (rf"{R} = revs\({R}\.l\)", un(lambda x: (s32(int.from_bytes((x & 0xffff).to_bytes(2, 'little'), 'big') << 16) >> 16))),
    (rf"{R} = rev8\({R}\)", un(lambda x: int.from_bytes(x.to_bytes(4, 'little'), 'big'))),
    (rf"{R} = clz\({R}\)", un(clz)),
    (rf"{R} = cls\({R}\)", un(cls)),
    (rf"{R} = abs\({R}\)", un(lambda x: abs(s32(x)))),
    (rf"{R} = ~{R}", un(lambda x: ~x)),
    (rf"{R} = -{R}", un(lambda x: -x)),
    (rf"{R} = {R}", un(lambda x: x)),
    (rf"{R} = ([su](?:max|min))\({R}, {R}\)", minmax),
    (rf"{R} \+= {R} \(brev\)", brevadd),
    (rf"{R}\.b(\d) = {IMM}", byteset),
    (rf"{R} = {R} (\||\^|&|& ~)\(1 << {R}\)", bitop),
    (rf"{R} = {R} (\||\^|&|& ~) \(1 << {R}\)", bitop),
    (rf"{R} = {IMM}", movi),
    (rf"{R} = {IMM} - {R}", rsub),
    (rf"{R} = {R}\.(b0|l) \(([su])\)", ext),
    (rf"{R} = ([su])extra\({R}, p:(\d+), l:(\d+)\)", extra),
]

line_re = re.compile(r"^\s*([0-9a-f]+):\s+((?:[0-9a-f]{2} )+)\s*\t?\s*(.*)$")
insns = {}
for path in listings:
    for l in open(path, errors="replace"):
        m = line_re.match(l)
        if m and "unknown" not in m[3] and not m[3].startswith("<"):
            insns.setdefault(m[2].replace(" ", ""), m[3].strip())

def regs_for(S):
    r = [rng.choice([rng.getrandbits(32), rng.getrandbits(8), ((1 << 32) - rng.getrandbits(8)) & M,
                     0x7fffffff, 0x80000000, rng.getrandbits(32) >> rng.randrange(32)]) for _ in range(16)]
    for a in S.amount_regs:
        r[a] = rng.randrange(64 if "_" in S.t else 32)
    return r

cases, skipped = [], collections.Counter()
for h, t in insns.items():
    S = Sem(t)
    if not S.fn:
        skipped[re.sub(r"\d+", "N", re.sub(r"<[^>]*>", "", t).replace("#", "").strip())] += 1
        continue
    for _ in range(N):
        r = regs_for(S)
        want = list(r)
        try:
            S.fn(want)
        except (KeyError, ValueError):   # an amount the text leaves undefined (junk decoded as code)
            continue
        cases.append((h, t, r, want))

with tempfile.NamedTemporaryFile("w", suffix=".txt", delete=False) as f:
    for h, t, r, _ in cases:
        f.write(h + " " + " ".join(f"{v:x}" for v in r) + " 0\n")
    inp = f.name
out = subprocess.run([emu, "--sem-run", inp], capture_output=True, text=True, check=True).stdout.splitlines()

bad = collections.defaultdict(list)
ok = 0
for (h, t, r, want), o in zip(cases, out):
    if o.startswith("err"):
        got = o
    else:
        got = [int(x, 16) for x in o.split()[:16]]
        if got == want:
            ok += 1
            continue
    shape = re.sub(r"\d+", "N", re.sub(r"<[^>]*>", "", t).replace("#", "").strip())
    bad[shape].append((h, t, r, want, got))

for shape, v in sorted(bad.items(), key=lambda kv: -len(kv[1])):
    encs = sorted({x[0] for x in v})
    print(f"[{shape}] {len(v)} runs, {len(encs)} encodings")
    for h, t, r, want, got in v[:2]:
        if isinstance(got, str):
            print(f"  {h:14} {t:36} {got}")
            continue
        diff = [k for k in range(16) if want[k] != got[k]]
        print(f"  {h:14} {t:36} " + "  ".join(f"r{k}: want {want[k]:x} got {got[k]:x}" for k in diff)
              + "   in: " + " ".join(f"r{k}={r[k]:x}" for k in sorted(set(int(x) for x in re.findall(r'r(\d+)', t)))))
print(f"\n{len(insns)} distinct instructions, {len(insns) - sum(skipped.values())} evaluated: "
      f"{len(cases)} runs, {ok} agree, {len(cases) - ok} differ in {len(bad)} shapes")
if "--skipped" in sys.argv:
    for s, n in skipped.most_common(40):
        print(f"  skipped {n:6}  {s}")
