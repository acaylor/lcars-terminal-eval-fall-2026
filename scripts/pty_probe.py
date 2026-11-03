#!/usr/bin/env python3
"""Controlled PTY runtime, layout, interaction, fallback and lifecycle probes.

Usage: pty_probe.py [--root EVALDIR] [--platform-tag linux|macos] [--quick] S01 [S02 ...]

For each snapshot it writes:
  artifacts/SXX/pty-probe[-<tag>].json      structured results
  artifacts/SXX/metric-ref-*.json           live OS reference values captured at screen time
  captures-fresh/SXX/[<tag>/]*.txt|.ansi|.svg|.png
"""
import argparse
import json
import os
import re
import signal
import subprocess
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import lcars_harness as H  # noqa: E402

PANIC_RE = re.compile(rb"panicked at|panic:|goroutine \d+ \[|RUST_BACKTRACE|SIGSEGV|fatal error:")
KEY_HINTS = {
    "help_?": re.compile(r"\?\s*[:=]?\s*(HELP|help|Help)|(HELP|help)\s*[:=]?\s*\?|\[\?\]|\?\s"),
    "quit_q": re.compile(r"\b[Qq]\b\s*[:=\-]?\s*(QUIT|quit|EXIT|exit)|(QUIT|EXIT)"),
    "pause_space": re.compile(r"SPACE|SPC|Space|PAUSE|pause|␣"),
    "proc_p": re.compile(r"\b[Pp]\b\s*[:=\-]?\s*(PROC|proc|DIAG|diag)|PROC|DIAG"),
    "tab": re.compile(r"TAB|Tab|⇥"),
}


def digits(text):
    return re.findall(r"\d+(?:[.:]\d+)*", text)


def changed_cells(a: H.ScreenSnap, b: H.ScreenSnap) -> int:
    if (a.cols, a.rows) != (b.cols, b.rows):
        return -1
    return sum(1 for y in range(a.rows) for x in range(a.cols) if a.cells[y][x] != b.cells[y][x])


def structure_mask(s: H.ScreenSnap):
    return [[H.color_hex(c[2], H.DEFAULT_BG) != H.DEFAULT_BG or c[4] or c[0] not in ("", " ")
             for c in row] for row in s.cells]


def structure_similarity(a, b) -> float:
    if (a.cols, a.rows) != (b.cols, b.rows):
        return -1.0
    ma, mb = structure_mask(a), structure_mask(b)
    same = sum(1 for y in range(a.rows) for x in range(a.cols) if ma[y][x] == mb[y][x])
    return round(same / (a.cols * a.rows), 3)


def floats_in(text):
    return [float(x) for x in re.findall(r"(?<![\d.])\d+\.\d+(?![\d.])", text)]


def ints_in(text):
    return [int(x) for x in re.findall(r"(?<![\d.])\d+(?![\d.])", text)]


def metric_checks_primary(text: str, ref: dict) -> dict:
    out = {}
    host = ref.get("hostname", "")
    out["hostname_present"] = host.lower() in text.lower() or (len(host) > 8 and host[:8].lower() in text.lower())
    out["time_hh_mm_ss_present"] = bool(re.search(r"\b\d{2}:\d{2}:\d{2}\b", text))
    hhmm = ref["t_wall"][11:16]
    out["time_matches_wall_hh_mm"] = hhmm in text
    out["date_present"] = bool(re.search(r"\d{4}-\d{2}-\d{2}|\b(JAN|FEB|MAR|APR|MAY|JUN|JUL|AUG|SEP|OCT|NOV|DEC)\w*\b",
                                         text, re.I)) or ref["t_wall"][:4] in text
    fl = floats_in(text)
    if isinstance(ref.get("loadavg"), list):
        out["loadavg_matches"] = [any(abs(f - la) <= 0.06 for f in fl) for la in ref["loadavg"]]
    if ref.get("mem_total_gib"):
        tot = ref["mem_total_gib"]
        nums = fl + [float(i) for i in ints_in(text)]
        out["mem_total_present"] = any(abs(n - tot) / tot < 0.03 or abs(n - tot * 1.0737) / tot < 0.03 for n in nums)
        used = ref["mem_used_gib"]
        out["mem_used_plausible"] = any(abs(n - used) < max(0.6, used * 0.15) for n in fl)
    if ref.get("root_disk"):
        rd = ref["root_disk"]
        ints = ints_in(text)
        out["disk_pct_plausible"] = any(abs(i - rd["used_pct_df"]) <= 2 or abs(i - rd["used_pct_of_total"]) <= 2
                                        for i in ints)
    if ref.get("uptime_s"):
        d = int(ref["uptime_s"] // 86400)
        out["uptime_days_present"] = bool(re.search(rf"\b0*{d}\s*(d|D|DAY|day)", text))
    return out


def metric_checks_diag(text: str, ref: dict) -> dict:
    out = {}
    out["os_present"] = bool(re.search(r"linux|darwin|macos|mac os", text, re.I))
    out["arch_present"] = bool(re.search(r"x86_64|amd64|x64|arm64|aarch64", text, re.I))
    out["pid_header_present"] = bool(re.search(r"\bPID\b", text))
    out["swap_label_present"] = bool(re.search(r"swap|SWP", text, re.I))
    pc = ref.get("process_count")
    if isinstance(pc, int):
        out["process_count_plausible"] = any(abs(i - pc) <= max(15, pc * 0.1) for i in ints_in(text))
    tops = ref.get("top_cpu_processes") or []
    names = [t["name"][:7] for t in tops[:5]]
    out["ref_top5_names_seen"] = sum(1 for n in names if n and n in text)
    pids = [str(t["pid"]) for t in tops[:5]]
    out["ref_top5_pids_seen"] = sum(1 for p in pids if re.search(rf"\b{p}\b", text))
    return out


class Prober:
    def __init__(self, root, sid, tag, quick=False, steps=None):
        self.root, self.sid, self.tag, self.quick, self.only = root, sid, tag, quick, steps
        cfg = json.load(open(os.path.join(root, "scripts", "targets.json")))[sid]
        self.cfg = cfg
        self.work = os.path.join(root, "work", sid)
        self.bin = os.path.join(self.work, cfg["bin"])
        self.art = os.path.join(root, "artifacts", sid)
        self.cap = os.path.join(root, "captures-fresh", sid) + ("" if tag == "linux" else f"/{tag}")
        os.makedirs(self.art, exist_ok=True)
        os.makedirs(self.cap, exist_ok=True)
        self.res = {"sid": sid, "platform": tag, "binary": cfg["bin"], "started": time.strftime("%F %T")}

    def launch(self, cols=120, rows=36, args=(), env=None, unset=()):
        return H.PtySession([self.bin, *args], cols, rows, env_extra=env, env_unset=unset, cwd=self.work)

    def cap_save(self, snap, name, s=None):
        H.save_capture(snap, os.path.join(self.cap, name))

    def finish(self, s, how="q", timeout=3.0):
        """Terminate a session via `how`, return lifecycle observations."""
        t0 = time.monotonic()
        before = H.termios_summary(s.termios_before)
        if how == "q":
            s.send(b"q")
        elif how == "esc":
            s.send(b"\x1b")
        elif how == "ctrl-c-byte":
            s.send(b"\x03")
        elif how == "sigint":
            s.signal(signal.SIGINT)
        elif how == "sigterm":
            s.signal(signal.SIGTERM)
        rc = s.wait_exit(timeout)
        elapsed = time.monotonic() - t0
        hung = rc is None
        time.sleep(0.2)
        tail = s.bytes_since(t0)
        try:
            after = H.termios_summary(s.termios_now())
        except (OSError, H.termios.error):
            # macOS revokes the slave once the session leader exits; termios restoration
            # is graded from the Linux run, so record "unknown" here instead of failing.
            after = None
        out = {
            "how": how, "exited": not hung, "exit_code": rc, "exit_seconds": round(elapsed, 3),
            "restore_seq": H.restore_check(tail), "tail_bytes": len(tail),
            "termios_restored": (before["raw"] == after["raw"]) if after else None,
            "termios_after": {k: v for k, v in after.items() if k != "raw"} if after else None,
            "termios_before": {k: v for k, v in before.items() if k != "raw"},
            "panic_text": bool(PANIC_RE.search(s.all_bytes())),
            "tail_text": tail[-300:].decode("utf-8", "replace"),
        }
        s.close(kill=hung)
        return out

    # ------------------------------------------------------------ probes
    def primary_session(self):
        r = {}
        s = self.launch()
        time.sleep(1.5)
        ref = H.reference()  # ~1 s window for rates/top procs on Linux
        time.sleep(max(0, 2.5 - (time.monotonic() - s.t_spawn)))
        prim = s.snapshot()
        H.dump_json(ref, os.path.join(self.art, f"metric-ref-primary-{self.tag}.json"))
        self.cap_save(prim, "primary-120x36")
        text = prim.text()
        r["alive_after_2_5s"] = s.alive()
        r["primary_blank_fraction"] = round(prim.blank_fraction(), 3)
        r["primary_distinct_colors"] = prim.distinct_colors()
        r["primary_non_ascii_glyphs"] = len(prim.non_ascii_chars())
        r["primary_has_PID_word"] = bool(re.search(r"\bPID\b", text))
        tops = [t["name"][:7] for t in (ref.get("top_cpu_processes") or [])[:5] if len(t["name"]) >= 4]
        r["primary_ref_top_names_seen"] = [n for n in tops if n in text]
        r["primary_metric_checks"] = metric_checks_primary(text, ref) if self.tag == "linux" else {}
        r["primary_key_hints"] = {k: bool(v.search(text)) for k, v in KEY_HINTS.items()}
        r["primary_sgr"] = H.sgr_stats(s.all_bytes())
        r["terminal_queries_answered"] = dict(s.queries_answered)

        s.send(b"p")
        time.sleep(0.5)
        ref2 = H.reference()
        time.sleep(0.1)
        diag = s.snapshot()
        H.dump_json(ref2, os.path.join(self.art, f"metric-ref-diagnostic-{self.tag}.json"))
        self.cap_save(diag, "diagnostic-120x36")
        r["diag_changed_cells_vs_primary"] = changed_cells(prim, diag)
        r["diag_metric_checks"] = metric_checks_diag(diag.text(), ref2) if self.tag == "linux" else {
            "pid_header_present": bool(re.search(r"\bPID\b", diag.text()))}
        r["diag_key_hints"] = {k: bool(v.search(diag.text())) for k, v in KEY_HINTS.items()}

        s.send(b"p")
        time.sleep(1.0)
        back = s.snapshot()
        r["p_toggle_back_structure_similarity_to_primary"] = structure_similarity(prim, back)
        r["p_toggle_back_has_PID"] = bool(re.search(r"\bPID\b", back.text()))

        if self.quick:
            r["quit"] = self.finish(s, "q")
            return r

        s.send(b"?")
        time.sleep(1.0)
        helps = s.snapshot()
        self.cap_save(helps, "help-120x36")
        r["help_changed_cells"] = changed_cells(back, helps)
        r["help_text_mentions"] = {k: bool(re.search(k, helps.text(), re.I)) for k in
                                   ["quit", "help", "pause", "space", "tab", "proc|diag"]}
        s.send(b"?")
        time.sleep(1.0)
        closed = s.snapshot()
        r["help_closed_structure_similarity_to_primary"] = structure_similarity(prim, closed)
        r["help_closed_still_has_help_words"] = (r["help_changed_cells"] > 0 and
                                                 changed_cells(closed, helps) < 20)

        # pause / resume
        s.send(b" ")
        time.sleep(0.4)
        pa = s.snapshot()
        time.sleep(2.6)
        pb = s.snapshot()
        self.cap_save(pb, "paused-120x36")
        r["pause"] = {
            "digits_equal_over_2_6s": digits(pa.text()) == digits(pb.text()),
            "text_equal_over_2_6s": pa.text() == pb.text(),
            "changed_cells_over_2_6s": changed_cells(pa, pb),
            "changed_digit_tokens": [(x, y) for x, y in zip(digits(pa.text()), digits(pb.text())) if x != y][:12],
            "pause_indicator_visible": bool(re.search(r"PAUSE|HOLD|FROZEN|STANDBY|SUSPEND", pb.text(), re.I)),
        }
        s.send(b" ")
        time.sleep(2.2)
        pc = s.snapshot()
        r["resume"] = {"digits_changed_after_resume": digits(pb.text()) != digits(pc.text()),
                       "changed_cells": changed_cells(pb, pc)}

        # tab cycling
        tabs = []
        prev = pc
        for i in range(4):
            s.send(b"\t")
            time.sleep(0.8)
            t = s.snapshot()
            if i == 0:
                self.cap_save(t, "tab1-120x36")
            tabs.append({"alive": s.alive(), "changed_cells_vs_prev": changed_cells(prev, t)})
            prev = t
        r["tab_cycle"] = tabs
        r["quit"] = self.finish(s, "q")
        r["output_seq_counts"] = H.seq_counts(s.all_bytes())
        return r

    def lifecycle(self):
        out = {}
        for how in ["q", "esc", "ctrl-c-byte", "sigint", "sigterm"]:
            s = self.launch()
            time.sleep(1.5)
            alive = s.alive()
            res = self.finish(s, how)
            res["alive_before_stimulus"] = alive
            out[how] = res
        return out

    def sizes(self):
        out = {}
        for cols, rows, name in [(80, 24, "primary-80x24"), (40, 10, "tiny-40x10"), (20, 5, "tiny-20x5")]:
            s = self.launch(cols, rows)
            time.sleep(2.5)
            snap = s.snapshot()
            self.cap_save(snap, name)
            d = {"alive": s.alive(), "exit_code": s.proc.poll(), "panic_text": bool(PANIC_RE.search(s.all_bytes())),
                 "non_blank_cells": sum(1 for row in structure_mask(snap) for c in row),
                 "blank_fraction": round(snap.blank_fraction(), 3)}
            if name == "primary-80x24":
                s.send(b"p")
                time.sleep(1.2)
                self.cap_save(s.snapshot(), "diagnostic-80x24")
                s.send(b"p")
                time.sleep(0.3)
            d["quit"] = {k: v for k, v in self.finish(s, "q").items() if k in ("exited", "exit_code", "restore_seq", "termios_restored")}
            out[name] = d

        # live resize transitions
        s = self.launch(120, 36)
        time.sleep(2.0)
        first = s.snapshot()
        seq = []
        for cols, rows, name in [(80, 24, "resize-80x24"), (30, 8, "resize-30x8"), (120, 36, "resize-back-120x36")]:
            s.resize(cols, rows)
            time.sleep(1.5)
            snap = s.snapshot()
            self.cap_save(snap, name)
            seq.append({"size": f"{cols}x{rows}", "alive": s.alive(),
                        "non_blank_cells": sum(1 for row in structure_mask(snap) for c in row)})
        last = s.snapshot()
        out["live_resize"] = {
            "steps": seq,
            "panic_text": bool(PANIC_RE.search(s.all_bytes())),
            "structure_similarity_back_vs_initial": structure_similarity(first, last),
            "quit": {k: v for k, v in self.finish(s, "q").items() if k in ("exited", "exit_code", "restore_seq", "termios_restored")},
        }
        return out

    def color_modes(self):
        variants = {
            "color256": dict(env={"TERM": "xterm-256color"}, unset=("COLORTERM",), name="color256-120x36"),
            "color16": dict(env={"TERM": "xterm"}, unset=("COLORTERM",), name="color16-120x36"),
            "no_color_env": dict(env={"NO_COLOR": "1"}, name="mono-nocolor-120x36"),
            "mono_documented": dict(args=self.cfg["mono"].get("args", ()), env=self.cfg["mono"].get("env"),
                                    name="mono-120x36"),
            "ascii_documented": dict(args=self.cfg["ascii"].get("args", ()), env=self.cfg["ascii"].get("env"),
                                     name="ascii-120x36"),
            "locale_C": dict(env={"LC_ALL": "C", "LANG": "C"}, name="locale-c-120x36"),
            "term_dumb": dict(env={"TERM": "dumb"}, unset=("COLORTERM",), name="term-dumb-120x36"),
        }
        out = {}
        for key, v in variants.items():
            s = self.launch(120, 36, args=v.get("args", ()), env=v.get("env"), unset=v.get("unset", ()))
            time.sleep(2.5)
            snap = s.snapshot()
            self.cap_save(snap, v["name"])
            raw = s.all_bytes()
            nonascii_raw = sum(1 for ch in raw.decode("utf-8", "replace") if ord(ch) > 126)
            out[key] = {
                "alive": s.alive(), "exit_code": s.proc.poll(),
                "sgr": H.sgr_stats(raw),
                "non_ascii_codepoints_in_stream": nonascii_raw,
                "non_ascii_glyph_kinds_on_screen": len(snap.non_ascii_chars()),
                "non_ascii_examples": "".join(list(snap.non_ascii_chars())[:20]),
                "distinct_colors_on_screen": snap.distinct_colors(),
                "non_blank_cells": sum(1 for row in structure_mask(snap) for c in row),
                "panic_text": bool(PANIC_RE.search(raw)),
                "stream_head": raw[:200].decode("utf-8", "replace") if not s.alive() else "",
            }
            self.finish(s, "q")
        return out

    def non_tty(self):
        out_p = os.path.join(self.art, f"nontty-stdout-{self.tag}.log")
        err_p = os.path.join(self.art, f"nontty-stderr-{self.tag}.log")
        t0 = time.monotonic()
        with open(out_p, "wb") as fo, open(err_p, "wb") as fe:
            p = subprocess.Popen([self.bin], stdin=subprocess.DEVNULL, stdout=fo, stderr=fe, cwd=self.work,
                                 env={**os.environ, "TERM": "xterm-256color", "COLORTERM": "truecolor"},
                                 start_new_session=True)
            try:
                rc = p.wait(timeout=3)
                timed_out = False
            except subprocess.TimeoutExpired:
                os.killpg(p.pid, signal.SIGKILL)
                rc = p.wait()
                timed_out = True
        so, se = open(out_p, "rb").read(), open(err_p, "rb").read()
        return {"exit_code": rc, "timed_out_3s": timed_out, "seconds": round(time.monotonic() - t0, 3),
                "stdout_bytes": len(so), "stdout_has_escape": b"\x1b" in so, "stderr_has_escape": b"\x1b" in se,
                "panic_text": bool(PANIC_RE.search(so + se)),
                "stdout_head": so[:400].decode("utf-8", "replace"), "stderr_head": se[:600].decode("utf-8", "replace")}

    def run(self):
        if not os.path.exists(self.bin):
            self.res["error"] = f"binary missing: {self.bin}"
            return self.res
        steps = [("primary_session", self.primary_session), ("lifecycle", self.lifecycle),
                 ("non_tty", self.non_tty)]
        if not self.quick:
            steps += [("sizes", self.sizes), ("color_modes", self.color_modes)]
        suffix = "" if self.tag == "linux" else f"-{self.tag}"
        out_path = os.path.join(self.art, f"pty-probe{suffix}.json")
        if self.only:  # re-run selected steps, merging into the existing results file
            steps = [(n, f) for n, f in steps if n in self.only]
            if os.path.exists(out_path):
                self.res = {**json.load(open(out_path)), "rerun_steps": sorted(self.only)}
        for name, fn in steps:
            try:
                self.res[name] = fn()
            except Exception as e:  # harness failure is recorded, never fatal for the batch
                import traceback
                self.res[name] = {"harness_error": repr(e), "trace": traceback.format_exc()}
        self.res["finished"] = time.strftime("%F %T")
        H.dump_json(self.res, out_path)
        return self.res


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", default=os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    ap.add_argument("--platform-tag", default="linux" if sys.platform.startswith("linux") else "macos")
    ap.add_argument("--quick", action="store_true")
    ap.add_argument("--steps", default="", help="comma list of steps to re-run and merge")
    ap.add_argument("ids", nargs="+")
    a = ap.parse_args()
    only = set(x for x in a.steps.split(",") if x) or None
    for sid in a.ids:
        t = time.monotonic()
        r = Prober(a.root, sid, a.platform_tag, a.quick, only).run()
        lc = r.get("lifecycle", {})
        brief = {k: (v.get("exited"), v.get("termios_restored"), v.get("restore_seq")) for k, v in lc.items()
                 if isinstance(v, dict)}
        print(f"{sid}: done in {time.monotonic()-t:.0f}s lifecycle={brief}", flush=True)


if __name__ == "__main__":
    main()
