#!/usr/bin/env python3
"""Create anonymized clean-room snapshots S01..S09 of the nine evaluated submissions.

Deterministic: the shuffle is seeded with a fixed string, so re-running produces the
same mapping. The mapping is written ONLY to evaluation/private/submission-map.tsv and is
never printed. Original directories are only read (rsync source), never modified.
"""
import json
import os
import random
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
EVAL = ROOT / "evaluation"
SEED = "lcars-v3-fall-2026-clean-room"

INCLUDED = [
    "deepseek-v4.1-flash-1017", "fable-5.1", "gemini-4", "gpt-6-astra", "gpt-6.1-sol",
    "grok-4.7", "kimi-k3.1-pro", "muse-wonder", "opus-5.5",
]
EXCLUDED_DIRS = {"opus-5", "gemini-3.8", "sonnet-5.5", "gpt-6-sol"}  # never touched

RSYNC_EXCLUDES = [
    "/.git/", "/target/", "/bin/", "/cost.txt", "/prompt-v3.md",
    "/captures/", "/artifacts/", "/docs/captures/",
]

CAPTURE_DIRS = ["captures", "artifacts", "docs/captures"]
CAPTURE_EXTS = {".txt", ".ansi", ".png", ".svg", ".html"}


def text_dims(path: Path):
    """Best-effort (cols, rows) of a plain-text capture."""
    try:
        lines = path.read_text(errors="replace").rstrip("\n").split("\n")
    except OSError:
        return None
    if not lines:
        return None
    return max(len(l) for l in lines), len(lines)


def classify(name: str):
    n = name.lower()
    if "secondary" in n or "diag" in n or "process" in n:
        return "secondary"
    if "primary" in n:
        return "primary"
    return None


def manifest_for(src: Path):
    caps = []
    for cd in CAPTURE_DIRS:
        d = src / cd
        if not d.is_dir():
            continue
        for f in sorted(d.rglob("*")):
            if f.is_file() and f.suffix.lower() in CAPTURE_EXTS:
                caps.append(f)
    found = {"primary": [], "secondary": []}
    for f in caps:
        kind = classify(f.name)
        if kind is None:
            continue
        rel = str(f.relative_to(src))
        n = f.name.lower()
        is120 = "120x36" in n
        if not is120 and f.suffix == ".txt":
            dims = text_dims(f)
            is120 = bool(dims and dims[1] == 36 and 110 <= dims[0] <= 120)
        if not is120 and f.suffix != ".txt":
            # sibling .txt with the same stem decides
            sib = f.with_suffix(".txt")
            dims = text_dims(sib) if sib.exists() else None
            is120 = bool(dims and dims[1] == 36 and 110 <= dims[0] <= 120)
        if is120 and "ascii" not in n and "mono" not in n:
            found[kind].append(rel)

    logs = []
    for cand in ["CAPTAINS_LOG.md", "CAPTAINS-LOG.md", "docs/CAPTAINS_LOG.md", "docs/CAPTAINS-LOG.md"]:
        if (src / cand).is_file():
            logs.append(cand)
    readme_section = False
    readme = src / "README.md"
    if readme.is_file():
        for line in readme.read_text(errors="replace").splitlines():
            if line.lstrip().startswith("#") and "captain" in line.lower():
                readme_section = True
    return {
        "primary_120x36_capture": bool(found["primary"]),
        "secondary_120x36_capture": bool(found["secondary"]),
        "primary_capture_files": found["primary"],
        "secondary_capture_files": found["secondary"],
        "captains_log_file": logs,
        "captains_log_readme_section": readme_section,
        "all_capture_files_stripped": [str(f.relative_to(src)) for f in caps],
    }


def find_prebuilt_binaries(dst: Path):
    out = []
    for f in dst.rglob("*"):
        if f.is_file():
            with open(f, "rb") as fh:
                head = fh.read(4)
            if head in (b"\x7fELF", b"\xcf\xfa\xed\xfe", b"\xfe\xed\xfa\xcf", b"\xca\xfe\xba\xbe"):
                out.append(f)
    return out


def main():
    for d in INCLUDED:
        if not (ROOT / d).is_dir():
            sys.exit(f"missing submission directory: {d}")
    order = INCLUDED[:]
    random.Random(SEED).shuffle(order)

    work = EVAL / "work"
    work.mkdir(parents=True, exist_ok=True)
    (EVAL / "private").mkdir(parents=True, exist_ok=True)

    mapping = []
    manifest = {}
    for i, d in enumerate(order, 1):
        sid = f"S{i:02d}"
        src = ROOT / d
        dst = work / sid
        if dst.exists():
            shutil.rmtree(dst)
        manifest[sid] = manifest_for(src)
        cmd = ["rsync", "-a"] + [f"--exclude={e}" for e in RSYNC_EXCLUDES] + [f"{src}/", f"{dst}/"]
        subprocess.run(cmd, check=True)
        removed = []
        for b in find_prebuilt_binaries(dst):
            removed.append(str(b.relative_to(dst)))
            b.unlink()
        manifest[sid]["prebuilt_binaries_removed"] = removed
        mapping.append((sid, d))
        (EVAL / "artifacts" / sid).mkdir(parents=True, exist_ok=True)
        (EVAL / "captures-fresh" / sid).mkdir(parents=True, exist_ok=True)

    with open(EVAL / "private" / "submission-map.tsv", "w") as fh:
        fh.write("anon_id\tdirectory\n")
        for sid, d in mapping:
            fh.write(f"{sid}\t{d}\n")
    with open(EVAL / "private" / "deliverables-manifest.json", "w") as fh:
        json.dump(manifest, fh, indent=2, sort_keys=True)
    print(f"created {len(mapping)} snapshots under {work} (mapping written privately)")


if __name__ == "__main__":
    main()
