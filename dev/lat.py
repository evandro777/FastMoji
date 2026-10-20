"""Measure hotkey -> window-mapped (title "*" matches any new window) latency and memory of a resident picker."""
import sys, time
from Xlib import X, XK, display
from Xlib.ext import xtest
key, title, pid, n = sys.argv[1], sys.argv[2], int(sys.argv[3]), int(sys.argv[4])
d = display.Display(); root = d.screen().root
root.change_attributes(event_mask=X.SubstructureNotifyMask)
d.sync()
def code(s): return d.keysym_to_keycode(XK.string_to_keysym(s))
def rss():
    for l in open(f"/proc/{pid}/smaps_rollup"):
        if l.startswith("Rss:"): r = int(l.split()[1])
        if l.startswith("Pss:"): p = int(l.split()[1])
    return r, p
def tap(*keys):
    for k in keys: xtest.fake_input(d, X.KeyPress, code(k))
    for k in reversed(keys): xtest.fake_input(d, X.KeyRelease, code(k))
    d.sync()
print("idle   RSS/PSS kB:", rss())
for i in range(n):
    while d.pending_events(): d.next_event()
    t0 = time.perf_counter(); tap('Control_L', 'Alt_L', key)
    t = None
    while time.perf_counter() - t0 < 5:
        if d.pending_events():
            ev = d.next_event()
            if ev.type == X.MapNotify:
                try:
                    p = ev.window.get_full_property(d.intern_atom("_NET_WM_NAME"), 0)
                    nm = (p.value.decode() if p else "") + " " + " ".join(ev.window.get_wm_class() or ())
                except Exception: nm = ""
                if title == "*" or title.lower() in nm.lower(): t = time.perf_counter() - t0; break
        else: time.sleep(0.0005)
    time.sleep(1.5)
    print(f"open {i}: {None if t is None else round(t*1000)} ms, shown RSS/PSS kB: {rss()}")
    tap('Escape'); time.sleep(1.5)
print("closed RSS/PSS kB:", rss())
