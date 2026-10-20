"""keep_resident = false: launch, search, Enter -> must paste AND the process must be gone soon after."""
import sys, subprocess, time; sys.path.insert(0,'dev'); from drive import *
def running(): return int(subprocess.run(['pgrep','-fc','^\\./target/fast/fastmoji'],capture_output=True,text=True).stdout.strip() or 0)
bad = 0; gone_after = []
for i in range(int(sys.argv[1])):
    focus_scratch_editor(); tap('Control_L','a'); tap('Delete'); tap('Control_L','s'); time.sleep(0.3)
    subprocess.Popen(['./target/fast/fastmoji']); time.sleep(1.2)
    typ('grin'); time.sleep(0.2); tap('Return'); t0 = time.time()
    while running() and time.time() - t0 < 4: time.sleep(0.02)
    gone_after.append(round(time.time() - t0, 2))
    time.sleep(0.3); tap('Control_L','s'); time.sleep(0.4)
    if not open('dev/scratch.txt').read().strip(): bad += 1; print(i, "NOT PASTED")
    if running(): bad += 1; print(i, "STILL RUNNING")
print(f"{sys.argv[1]} runs, {bad} problems; process gone after (s): min {min(gone_after)} max {max(gone_after)}")
