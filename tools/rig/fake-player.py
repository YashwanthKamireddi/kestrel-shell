#!/usr/bin/env python3
"""A stand-in music player for the test rig: publishes one song over MPRIS on the session bus
it is started on (rig.sh bus python3 fake-player.py ART), with artwork, length and a running
position, and answers play/pause, next, previous, seek and set-position. Makes no sound."""

import sys
import time

import gi

gi.require_version("Gio", "2.0")
from gi.repository import Gio, GLib  # noqa: E402

XML = """
<node>
  <interface name="org.mpris.MediaPlayer2">
    <method name="Raise"/><method name="Quit"/>
    <property name="Identity" type="s" access="read"/>
    <property name="CanQuit" type="b" access="read"/>
    <property name="CanRaise" type="b" access="read"/>
  </interface>
  <interface name="org.mpris.MediaPlayer2.Player">
    <method name="PlayPause"/><method name="Play"/><method name="Pause"/>
    <method name="Next"/><method name="Previous"/>
    <method name="Seek"><arg type="x" direction="in"/></method>
    <method name="SetPosition"><arg type="o" direction="in"/><arg type="x" direction="in"/></method>
    <property name="PlaybackStatus" type="s" access="read"/>
    <property name="Metadata" type="a{sv}" access="read"/>
    <property name="Position" type="x" access="read"/>
    <property name="CanSeek" type="b" access="read"/>
    <property name="CanGoNext" type="b" access="read"/>
    <property name="CanGoPrevious" type="b" access="read"/>
    <property name="CanPlay" type="b" access="read"/>
    <property name="CanPause" type="b" access="read"/>
    <property name="CanControl" type="b" access="read"/>
    <property name="Volume" type="d" access="read"/>
  </interface>
</node>
"""

SONGS = [("Midnight City", "M83"), ("Blinding Lights", "The Weeknd"), ("Cry", "Charlie Puth")]
LENGTH = 240_000_000  # µs

state = {"song": 0, "playing": True, "pos": 30_000_000, "since": time.monotonic()}
art = sys.argv[1] if len(sys.argv) > 1 else ""


def position():
    p = state["pos"]
    if state["playing"]:
        p += int((time.monotonic() - state["since"]) * 1_000_000)
    return min(p, LENGTH)


def set_position(p):
    state["pos"] = max(0, min(p, LENGTH))
    state["since"] = time.monotonic()


def metadata():
    title, artist = SONGS[state["song"]]
    return {
        "mpris:trackid": GLib.Variant("o", f"/rig/track/{state['song']}"),
        "mpris:length": GLib.Variant("x", LENGTH),
        "xesam:title": GLib.Variant("s", title),
        "xesam:artist": GLib.Variant("as", [artist]),
        "mpris:artUrl": GLib.Variant("s", f"file://{art}" if art else ""),
    }


def props(iface):
    if iface == "org.mpris.MediaPlayer2":
        return {
            "Identity": GLib.Variant("s", "Rig Player"),
            "CanQuit": GLib.Variant("b", False),
            "CanRaise": GLib.Variant("b", False),
        }
    return {
        "PlaybackStatus": GLib.Variant("s", "Playing" if state["playing"] else "Paused"),
        "Metadata": GLib.Variant("a{sv}", metadata()),
        "Position": GLib.Variant("x", position()),
        "CanSeek": GLib.Variant("b", True),
        "CanGoNext": GLib.Variant("b", True),
        "CanGoPrevious": GLib.Variant("b", True),
        "CanPlay": GLib.Variant("b", True),
        "CanPause": GLib.Variant("b", True),
        "CanControl": GLib.Variant("b", True),
        "Volume": GLib.Variant("d", 1.0),
    }


conn = None


def changed():
    conn.emit_signal(
        None,
        "/org/mpris/MediaPlayer2",
        "org.freedesktop.DBus.Properties",
        "PropertiesChanged",
        GLib.Variant(
            "(sa{sv}as)",
            ("org.mpris.MediaPlayer2.Player", props("org.mpris.MediaPlayer2.Player"), []),
        ),
    )


def on_call(_c, _sender, _path, _iface, method, params, inv):
    if method in ("PlayPause", "Play", "Pause"):
        set_position(position())
        state["playing"] = {"PlayPause": not state["playing"], "Play": True, "Pause": False}[method]
    elif method in ("Next", "Previous"):
        state["song"] = (state["song"] + (1 if method == "Next" else -1)) % len(SONGS)
        set_position(0)
    elif method == "Seek":
        set_position(position() + params.unpack()[0])
    elif method == "SetPosition":
        set_position(params.unpack()[1])
    inv.return_value(None)
    changed()


def on_get(_c, _sender, _path, iface, prop):
    return props(iface).get(prop)


def on_bus(c, _name):
    global conn
    conn = c
    info = Gio.DBusNodeInfo.new_for_xml(XML)
    for i in info.interfaces:
        c.register_object("/org/mpris/MediaPlayer2", i, on_call, on_get, None)


Gio.bus_own_name(
    Gio.BusType.SESSION,
    "org.mpris.MediaPlayer2.rigplayer",
    Gio.BusNameOwnerFlags.NONE,
    on_bus,
    None,
    None,
)
GLib.MainLoop().run()
