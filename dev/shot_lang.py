"""Starts FastMoji in an isolated profile with LANG=<lang>, types <query> and screenshots the picker (FastMoji's own
window only). usage: shot_lang.py <lang> <ascii query> <out-prefix>"""
import os, subprocess, sys, time
from drive import find_window, shot, typ

lang, query, out = sys.argv[1:4]
root = os.path.dirname(os.path.abspath(__file__))
env = dict(os.environ, XDG_DATA_HOME=f"{root}/data", XDG_RUNTIME_DIR=f"{root}/run", LANG=lang, LC_ALL=lang)
proc = subprocess.Popen([f"{root}/../target/fast/fastmoji"], env=env)
try:
    time.sleep(2.5)
    win = find_window()
    assert win, "no FastMoji window"
    typ(query)
    time.sleep(0.5)
    shot(f"{out}-main.png", win)
finally:
    proc.kill()
