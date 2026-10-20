"""Dev helpers: synthetic X11 input and window-only screenshots (never grabs the whole screen)."""
import subprocess, time
from Xlib import X, XK, display
from Xlib.ext import xtest
d = display.Display()
def code(name): return d.keysym_to_keycode(XK.string_to_keysym(name))
def tap(*keys):
    for k in keys: xtest.fake_input(d, X.KeyPress, code(k))
    for k in reversed(keys): xtest.fake_input(d, X.KeyRelease, code(k))
    d.sync(); time.sleep(0.04)
def typ(text):
    for c in text: tap({' ': 'space'}.get(c, c))
def click(x, y, button=1):
    xtest.fake_input(d, X.MotionNotify, False, x=x, y=y); d.sync(); time.sleep(0.05)
    xtest.fake_input(d, X.ButtonPress, button); xtest.fake_input(d, X.ButtonRelease, button); d.sync(); time.sleep(0.05)
def find_window(klass="fastmoji", min_w=200):
    """(x, y, w, h) of the first mapped window with the given WM_CLASS and a real size."""
    out = subprocess.run(['xwininfo', '-root', '-tree'], capture_output=True, text=True).stdout
    for line in out.splitlines():
        if f'("{klass}"' in line and 'x' in line:
            wid = line.split()[0]
            info = subprocess.run(['xwininfo', '-id', wid], capture_output=True, text=True).stdout
            g = {k: int(next(l for l in info.splitlines() if k in l).split()[-1]) for k in
                 ('Absolute upper-left X', 'Absolute upper-left Y', 'Width', 'Height')}
            if g['Width'] >= min_w and 'IsViewable' in info:
                return g['Absolute upper-left X'], g['Absolute upper-left Y'], g['Width'], g['Height']
def shot(path, win):
    x, y, w, h = win
    subprocess.run(['ffmpeg', '-loglevel', 'error', '-y', '-f', 'x11grab', '-draw_mouse', '0', '-video_size', f'{w}x{h}',
                    '-i', f':0.0+{x},{y}', '-frames:v', '1', path])

def find_window_by_title(fragment, min_w=200):
    """Geometry of the first mapped window whose title contains `fragment` (never a bare class match)."""
    out = subprocess.run(['xwininfo', '-root', '-tree'], capture_output=True, text=True).stdout
    for line in out.splitlines():
        if fragment in line and '"' in line:
            wid = line.split()[0]
            info = subprocess.run(['xwininfo', '-id', wid], capture_output=True, text=True).stdout
            if 'IsViewable' not in info: continue
            g = {k: int(next(l for l in info.splitlines() if k in l).split()[-1]) for k in
                 ('Absolute upper-left X', 'Absolute upper-left Y', 'Width', 'Height')}
            if g['Width'] >= min_w:
                return g['Absolute upper-left X'], g['Absolute upper-left Y'], g['Width'], g['Height']

def active_title():
    root = d.screen().root
    prop = root.get_full_property(d.intern_atom('_NET_ACTIVE_WINDOW'), X.AnyPropertyType)
    if not prop or not prop.value: return ''
    w = d.create_resource_object('window', prop.value[0])
    name = w.get_full_property(d.intern_atom('_NET_WM_NAME'), 0)
    return name.value.decode() if name else ''
def focus_scratch_editor():
    """Click into the scratch xed window and REFUSE to continue unless it is really the active window.
    Ctrl+A / Delete / Ctrl+S are destructive: sent to another window (a file manager!) they delete files."""
    win = find_window_by_title('scratch.txt')
    if not win: raise SystemExit("scratch xed window not found (start: xed --standalone dev/scratch.txt)")
    click(win[0] + win[2] // 2, win[1] + win[3] // 2); time.sleep(0.4)
    title = active_title()
    if 'scratch.txt' not in title:
        raise SystemExit(f"refusing to send destructive keys: active window is {title!r}")
