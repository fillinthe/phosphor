# dmgbuild settings for the Mac installer window (scripts/make-dmg.sh passes app, background).
import os

app = defines["app"]  # noqa: F821 (dmgbuild provides `defines`)
background = defines["background"]  # noqa: F821
name = os.path.basename(app)

files = [app]
hide_extensions = [name]
symlinks = {"Applications": "/Applications"}
format = "UDZO"
filesystem = "HFS+"

# The window: the picture is 660 x 420 points; the frame adds the title bar (28). Icons only (no toolbar, sidebar or
# status bar); the picture leaves its bottom strip empty in case Finder's path bar is on everywhere.
window_rect = ((200, 160), (660, 448))
default_view = "icon-view"
show_toolbar = False
show_sidebar = False
show_status_bar = False
show_tab_view = False
show_pathbar = False
icon_size = 128
text_size = 13
# Where macos/dmg/background.html draws the pads (centres, in points).
icon_locations = {name: (170, 200), "Applications": (490, 200)}
arrange_by = None
include_icon_view_settings = True
