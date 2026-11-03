#!/usr/bin/env python3
"""Objective static index of each snapshot (a starting point for manual source review).

Writes artifacts/SXX/static-scan.json and static-scan.txt with file:line hits for:
subprocess spawning, unsafe/cgo, TODO-style placeholders, panic profile, mouse/kitty/sixel
/config-file scope-creep markers, per-file LOC, test function counts and dependency counts.
"""
import json
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PATTERNS = {
    "subprocess": r"Command::new|std::process::Command|exec\.Command|os/exec|syscall\.Exec|\bpopen\(|\bsystem\(|posix_spawn|execv",
    "unsafe_block": r"\bunsafe\s*\{|\bunsafe\s+fn\b|\bunsafe\s+impl\b",
    "cgo": r'import "C"|#cgo|C\.[a-z_]+\(',
    "go_unsafe": r'"unsafe"|unsafe\.Pointer',
    "placeholder": r"\bTODO\b|\bFIXME\b|\bXXX\b|unimplemented!|todo!\(|\bstub\b|placeholder|not implemented",
    "panic_hook": r"panic::set_hook|take_hook|recover\(\)",
    "signal_handling": r"SIGTERM|SIGINT|signal-hook|signal\.Notify|sigaction|signal\(libc",
    "scope_mouse": r"EnableMouseCapture|\?1000h|\?1006h|mouse",
    "scope_protocols": r"kitty|sixel|iTerm|\x1b_G",
    "scope_config": r"\.toml\"|\.yaml|\.json\"|config_dir|XDG_CONFIG|dirs::config",
    "scope_proc_mgmt": r"\bkill\(|renice|setpriority|SIGKILL",
    "full_clear": r"\\x1b\[2J|\\033\[2J|\\u\{1b\}\[2J|Clear\(ClearType::All\)|ClearType::All|\[2J",
    "dt_assumption": r"elapsed|Instant::now|time\.Since|monotonic|as_secs_f64",
}
SRC_EXT = (".rs", ".go")


def files(work):
    out = []
    for dp, dn, fn in os.walk(work):
        dn[:] = [d for d in dn if d not in ("target", "bin", ".git", "vendor")]
        for f in fn:
            if f.endswith(SRC_EXT):
                out.append(os.path.join(dp, f))
    return sorted(out)


def scan(sid):
    work = os.path.join(ROOT, "work", sid)
    res = {"sid": sid, "files": {}, "hits": {k: [] for k in PATTERNS}}
    tests = 0
    for f in files(work):
        rel = os.path.relpath(f, work)
        lines = open(f, errors="replace").read().splitlines()
        res["files"][rel] = len(lines)
        for i, line in enumerate(lines, 1):
            for k, pat in PATTERNS.items():
                if re.search(pat, line, re.I if k.startswith("scope") or k == "placeholder" else 0):
                    res["hits"][k].append(f"{rel}:{i}: {line.strip()[:140]}")
            if re.match(r"\s*#\[test\]", line) or re.match(r"\s*func Test\w+\(t \*testing\.T\)", line):
                tests += 1
    res["test_functions"] = tests
    res["total_loc"] = sum(res["files"].values())
    if os.path.exists(os.path.join(work, "Cargo.toml")):
        toml = open(os.path.join(work, "Cargo.toml")).read()
        m = re.search(r"panic\s*=\s*\"(\w+)\"", toml)
        res["panic_profile"] = m.group(1) if m else "unwind (default)"
        try:
            tree = subprocess.run(["cargo", "tree", "--prefix", "none", "-e", "normal", "--target", "all"],
                                  cwd=work, capture_output=True, text=True, timeout=120).stdout
            pk = sorted(set(l.split(" (")[0].strip() for l in tree.splitlines() if l.strip()))
            res["dependency_crates_all_targets"] = pk
            res["dependency_crate_count"] = len(pk) - 1
        except Exception as e:
            res["dependency_error"] = repr(e)
    else:
        try:
            mods = subprocess.run(["go", "list", "-m", "all"], cwd=work, capture_output=True, text=True,
                                  timeout=120, env={**os.environ, "GOTOOLCHAIN": "local"}).stdout.split("\n")
            mods = [m for m in mods if m.strip()]
            res["go_modules"] = mods
            res["dependency_crate_count"] = len(mods) - 1
        except Exception as e:
            res["dependency_error"] = repr(e)
    return res


def main():
    ids = sys.argv[1:] or [f"S{i:02d}" for i in range(1, 10)]
    for sid in ids:
        r = scan(sid)
        art = os.path.join(ROOT, "artifacts", sid)
        with open(os.path.join(art, "static-scan.json"), "w") as fh:
            json.dump(r, fh, indent=2)
        with open(os.path.join(art, "static-scan.txt"), "w") as fh:
            fh.write(f"{sid}: {r['total_loc']} LOC in {len(r['files'])} files, {r['test_functions']} test fns, "
                     f"deps={r.get('dependency_crate_count')} panic={r.get('panic_profile', 'n/a')}\n\n")
            fh.write("FILES\n" + "\n".join(f"  {v:6d} {k}" for k, v in r["files"].items()) + "\n")
            for k, v in r["hits"].items():
                fh.write(f"\n{k.upper()} ({len(v)})\n" + "\n".join("  " + h for h in v[:400]) + "\n")
        print(f"{sid}: loc={r['total_loc']} tests={r['test_functions']} deps={r.get('dependency_crate_count')} "
              f"subproc={len(r['hits']['subprocess'])} unsafe={len(r['hits']['unsafe_block'])} "
              f"cgo={len(r['hits']['cgo'])} todo={len(r['hits']['placeholder'])} panic={r.get('panic_profile', '-')}")


if __name__ == "__main__":
    main()
