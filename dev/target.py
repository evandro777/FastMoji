import gi
gi.require_version("Gtk","4.0")
from gi.repository import Gtk
def act(app):
    w=Gtk.ApplicationWindow(application=app,title="PASTE-TARGET",default_width=300,default_height=80)
    e=Gtk.Entry(); w.set_child(e)
    e.connect("changed", lambda x: print("ENTRY:", repr(x.get_text()), flush=True))
    w.present(); e.grab_focus()
a=Gtk.Application(application_id="x.pastetarget"); a.connect("activate",act); a.run()
