#!/usr/bin/env python3
"""A plain maximized window, so screenshots of the notch show nothing else on the screen."""
import gi
gi.require_version('Gtk', '4.0')
from gi.repository import Gtk

def activate(app):
    w = Gtk.ApplicationWindow(application=app, decorated=False)
    css = Gtk.CssProvider()
    css.load_from_string("window { background: #cfd4dc; }")
    Gtk.StyleContext.add_provider_for_display(w.get_display(), css, 800)
    w.maximize()
    w.present()

app = Gtk.Application(application_id='local.nusa.backdrop')
app.connect('activate', activate)
app.run(None)
