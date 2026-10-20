import sys; sys.path.insert(0,'dev'); from drive import *
tw = find_window("x.pastetarget", 100) or find_window("python3", 100)
print("target", tw)
click(tw[0]+100, tw[1]+40); time.sleep(0.6)
tap('Control_L','Alt_L','e'); time.sleep(0.9)       # global hotkey -> picker
w = find_window(); print("picker", w)
typ("kiss"); time.sleep(0.5); shot('dev/s2.png', w)
tap('Down'); tap('Right'); time.sleep(0.3); shot('dev/s3.png', w)
tap('Return'); time.sleep(1.0)                        # copy + hide + paste
print("picker after enter:", find_window())
