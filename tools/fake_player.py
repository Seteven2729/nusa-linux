#!/usr/bin/env python3
"""A fake MPRIS player for testing Nusa without touching the user's real player.

    tools/fake_player.py --title "Let's not fall in love" --artist BIGBANG --length 211 \
        --cover /tmp/cover.png [--bus fakeplayer] [--identity Spotify] [--status Playing]

Pair it with NUSA_ONLY_PLAYER=fake (debug builds) so Nusa ignores every other player.
A real song title makes lrclib return real lyrics. Position advances from 1:15.
"""
import argparse, time, gi
gi.require_version('Gio', '2.0')
from gi.repository import Gio, GLib

ap = argparse.ArgumentParser()
ap.add_argument('--title', required=True)
ap.add_argument('--artist', required=True)
ap.add_argument('--length', type=int, required=True, help='seconds')
ap.add_argument('--cover', required=True, help='path to an image')
ap.add_argument('--bus', default='fakeplayer', help='suffix after org.mpris.MediaPlayer2.')
ap.add_argument('--identity', default='Fake Player')
ap.add_argument('--status', default='Playing', choices=['Playing', 'Paused', 'Stopped'])
a = ap.parse_args()

PLAYER = '''<node><interface name="org.mpris.MediaPlayer2.Player">
<method name="PlayPause"/><method name="Next"/><method name="Previous"/>
<method name="SetPosition"><arg type="o" direction="in"/><arg type="x" direction="in"/></method>
<property name="PlaybackStatus" type="s" access="read"/>
<property name="Metadata" type="a{sv}" access="read"/>
<property name="Position" type="x" access="read"/></interface></node>'''
ROOT = '<node><interface name="org.mpris.MediaPlayer2"><property name="Identity" type="s" access="read"/></interface></node>'
start = time.time() - 75

def player_prop(conn, sender, path, iface, prop):
    if prop == 'PlaybackStatus':
        return GLib.Variant('s', a.status)
    if prop == 'Position':
        return GLib.Variant('x', int((time.time() - start) * 1e6))
    return GLib.Variant('a{sv}', {
        'xesam:title': GLib.Variant('s', a.title),
        'xesam:artist': GLib.Variant('as', [a.artist]),
        'mpris:length': GLib.Variant('x', a.length * 1_000_000),
        'mpris:trackid': GLib.Variant('o', '/fake/track/1'),
        'mpris:artUrl': GLib.Variant('s', 'file://' + a.cover)})

def on_bus(conn, name):
    conn.register_object('/org/mpris/MediaPlayer2', Gio.DBusNodeInfo.new_for_xml(PLAYER).interfaces[0],
                         lambda *x: x[-1].return_value(None), player_prop, None)
    conn.register_object('/org/mpris/MediaPlayer2', Gio.DBusNodeInfo.new_for_xml(ROOT).interfaces[0],
                         None, lambda *x: GLib.Variant('s', a.identity), None)

Gio.bus_own_name(Gio.BusType.SESSION, 'org.mpris.MediaPlayer2.' + a.bus, 0, on_bus, None, None)
GLib.MainLoop().run()
