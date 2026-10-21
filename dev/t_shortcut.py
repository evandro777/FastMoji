"""Shortcut recorder scenarios against a running resident fastmoji (isolated env, see AGENTS.md, shortcut = Ctrl+Alt+period).
Usage: python3 dev/t_shortcut.py   (restores settings.toml's shortcut when done)"""
import sys; sys.path.insert(0, 'dev'); from drive import *
CONF = 'dev/data/FastMoji/settings.toml'
def conf(): return next(l.split('=')[1].strip().strip('"') for l in open(CONF) if l.startswith('shortcut'))
def hold(k, down):
    from Xlib import X; from Xlib.ext import xtest
    xtest.fake_input(d, X.KeyPress if down else X.KeyRelease, code(k)); d.sync(); time.sleep(0.25)
fails = []
def check(name, ok):
    print(('ok   ' if ok else 'FAIL '), name)
    if not ok: fails.append(name)
def ours(): return find_window() and 'FastMoji' in active_title()
start = conf()
tap('Control_L', 'Alt_L', 'period'); time.sleep(1.2)
w = find_window(); assert w and ours(), 'open FastMoji first'
click(w[0] + w[2] - 28, w[1] + 65); time.sleep(0.5)       # preferences
rec = lambda: (click(w[0] + 345, w[1] + 314), time.sleep(0.8))
rec(); tap('Escape'); time.sleep(0.3)
check('Esc cancels and keeps the shortcut', conf() == start and ours())
rec(); tap('Control_L', 'Alt_L', 'period'); time.sleep(1.0)
check('pressing the current shortcut keeps it, window stays open', conf() == start and ours())
check('recording ended after that (a key now does not record)', (tap('Control_L', 'Alt_L', 'x'), time.sleep(0.4), conf() == start)[2])
rec(); hold('Control_L', 1); hold('Alt_L', 1); hold('Alt_L', 0); hold('Control_L', 0)
check('modifiers alone do not end recording', conf() == start and ours())
rec(); tap('Control_L', 'Alt_L', '7'); time.sleep(1.0)
check('Ctrl+Alt+7 recorded', conf() == 'Ctrl+Alt+7')
rec(); tap('Control_L', 'Shift_L', 'k'); time.sleep(1.0)
check('Ctrl+Shift+K recorded', conf() == 'Ctrl+Shift+K')
rec(); tap('Control_L', 'F5'); time.sleep(1.0)
check('Ctrl+F5 recorded', conf() == 'Ctrl+F5')
# restore through the UI so the grab matches the file again
rec(); tap('Control_L', 'Alt_L', 'period'); time.sleep(1.0)
check('restored the original shortcut', conf() == start)
tap('Escape'); time.sleep(0.3); tap('Escape')
sys.exit(1 if fails else 0)
