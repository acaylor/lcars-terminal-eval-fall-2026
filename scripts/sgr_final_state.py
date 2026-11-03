#!/usr/bin/env python3
"""Supplementary lifecycle check: is the *last* SGR sequence in the whole output stream a
reset (i.e. are attributes left at default when the program hands the terminal back)?
Usage: sgr_final_state.py S01 [S02 ...]  -> artifacts/SXX/sgr-final-state.json"""
import json, os, re, sys, time, signal
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import lcars_harness as H
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
T = json.load(open(os.path.join(ROOT, "scripts", "targets.json")))
for sid in sys.argv[1:]:
    out = {}
    for how in ("q", "sigterm"):
        s = H.PtySession([os.path.join(ROOT, "work", sid, T[sid]["bin"])], 120, 36, cwd=os.path.join(ROOT, "work", sid))
        time.sleep(1.5)
        s.send(b"q") if how == "q" else s.signal(signal.SIGTERM)
        s.wait_exit(3); time.sleep(0.2)
        data = s.all_bytes(); s.close()
        sgrs = list(H.SGR_RE.finditer(data))
        last = sgrs[-1].group(0) if sgrs else b""
        params = last[2:-1]
        out[how] = {"last_sgr": last.decode(), "last_sgr_is_reset": params in (b"", b"0"),
                    "bytes_after_last_sgr": data[sgrs[-1].end():].decode("utf-8", "replace")[-120:] if sgrs else ""}
    H.dump_json(out, os.path.join(ROOT, "artifacts", sid, "sgr-final-state.json"))
    print(sid, {k: (v["last_sgr"].encode("unicode_escape").decode(), v["last_sgr_is_reset"]) for k, v in out.items()})
