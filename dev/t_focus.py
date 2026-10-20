import sys; sys.path.insert(0,'dev'); from drive import *
def focus_info():
    f = d.get_input_focus().focus
    try:
        cls = f.get_wm_class(); name = f.get_full_property(d.intern_atom("_NET_WM_NAME"),0)
        return f.id, cls, name.value if name else None
    except Exception as e: return f, str(e)
bad = 0
for i in range(int(sys.argv[1])):
    tap('Control_L','Alt_L','e'); time.sleep(1.0)
    fi = focus_info(); vis = find_window()
    tap('Escape'); time.sleep(0.8)
    still = find_window()
    if still:
        bad += 1; print(i, "ESC FAILED; x focus =", fi, "visible=", vis); tap('Control_L','Alt_L','e'); time.sleep(0.8)
print("failures", bad)
