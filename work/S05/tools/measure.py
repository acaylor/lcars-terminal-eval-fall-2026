"""Measure startup, size, idle CPU/RSS of bin/lcars."""
import fcntl, os, pty, statistics, struct, subprocess, termios, time

N = 15
times = []
for _ in range(N):
    t0 = time.perf_counter()
    subprocess.run(["./bin/lcars", "--once"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    times.append((time.perf_counter() - t0) * 1000)
print(f"startup --once median {statistics.median(times):.1f} ms  min {min(times):.1f}  max {max(times):.1f}  (n={N})")
sz = os.path.getsize("bin/lcars")
print(f"size {sz} bytes ({sz/1048576:.2f} MiB)")

pid, fd = pty.fork()
if pid == 0:
    os.environ["TERM"] = "xterm-256color"; os.environ["COLORTERM"] = "truecolor"
    os.execvp("./bin/lcars", ["lcars"])
fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", 36, 120, 0, 0))
time.sleep(4.0)
st = open(f"/proc/{pid}/stat").read().rsplit(")", 1)[1].split()
clk = os.sysconf("SC_CLK_TCK")
tot = (int(st[11]) + int(st[12])) / clk
rss = [l for l in open(f"/proc/{pid}/status").read().splitlines() if l.startswith("VmRSS")][0]
print(f"idle-4s cpu {tot:.3f}s ({tot/4*100:.2f}% of one core, tick-quantized); {rss.strip()}")
os.write(fd, b"q"); time.sleep(0.3)
try: os.kill(pid, 9)
except Exception: pass
os.waitpid(pid, 0)
print("OK")
