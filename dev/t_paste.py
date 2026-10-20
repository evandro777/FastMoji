"""Paste reliability: pick an emoji with the keyboard (or mouse) into xed and read it back."""
import sys, subprocess; sys.path.insert(0,'dev'); from drive import *
n = int(sys.argv[1]); mouse = len(sys.argv) > 2 and sys.argv[2] == 'mouse'
win = (0, 32, 1280, 688)
fails = 0
MANUAL = 'manual' in sys.argv
for i in range(n):
    focus_scratch_editor()
    tap('Control_L','a'); tap('Delete'); tap('Control_L','s'); time.sleep(0.4)
    tap('Control_L','Alt_L','y'); time.sleep(0.5)
    typ("grin"); time.sleep(0.25)
    if mouse:
        w = None
        for _ in range(10):
            w = find_window()
            if w: break
            time.sleep(0.1)
        if w: click(w[0]+28, w[1]+150)
        else: print("picker not visible")
    else:
        tap('Return')
    time.sleep(0.7)
    if MANUAL: tap('Control_L','v'); time.sleep(0.3)
    tap('Control_L','s'); time.sleep(0.4)                       # save, then read the file back
    got = open('dev/scratch.txt').read()
    if not got.strip():
        fails += 1
        clip = subprocess.run(['xclip','-o','-selection','clipboard'],capture_output=True,text=True).stdout
        shot('dev/fail.png', (0,32,1280,400)); print(i, "NOT PASTED; clipboard now:", repr(clip), "file:", repr(got))
print(f"{n} runs, {fails} failures")
