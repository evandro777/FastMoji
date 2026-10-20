import sys, subprocess; sys.path.insert(0,'dev'); from drive import *
fails=0
for i in range(20):
    focus_scratch_editor()
    tap('Control_L','a'); tap('Delete'); tap('Control_L','s'); time.sleep(0.3)
    subprocess.run(['xclip','-selection','clipboard','-i'],input='X',text=True); time.sleep(0.2)
    tap('Control_L','v'); time.sleep(0.3)
    tap('Control_L','s'); time.sleep(0.4)
    if not open('dev/scratch.txt').read().strip(): fails+=1; print(i,"fail")
print("xclip-owner paste:", 20, "runs", fails, "failures")
