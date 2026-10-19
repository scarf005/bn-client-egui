#!/bin/bash
# Source this file to drive a GUI client on a private Xvfb, for screenshots and xdotool checks.
#
#   source tools/xvfb.sh
#   xvfb_start                              # private display, only PIDs we started are ever killed
#   xvfb_launch target/debug/bn-client-egui --fake
#   xvfb_wait_window "Bright Nights"        # mapped
#   xvfb_wait_frame                         # first frame presented, not just a mapped black window
#   xvfb_focus "Bright Nights"              # no window manager: keys reach nobody without this
#   xdotool mousemove 40 40 click 1; xvfb_shot out.png
#   xvfb_stop
#
# Why: lanes shared display numbers and `pkill -x Xvfb`, so one lane's cleanup blanked another's
# screenshots. Here the display is picked from free numbers, Xvfb runs from a private copy of the binary
# (no `pkill -x Xvfb` of anyone else matches it) and xvfb_stop kills only what this shell started.
# Needs Xvfb, xdotool and ImageMagick (`import`). Scratch lives under $XVFB_SCRATCH (default ~/.cache/bn-xvfb).

XVFB_SCRATCH=${XVFB_SCRATCH:-$HOME/.cache/bn-xvfb}
XVFB_SIZE=${XVFB_SIZE:-1280x900}
XVFB_PIDS=()

xvfb_start() {
  mkdir -p "$XVFB_SCRATCH"
  local bin="$XVFB_SCRATCH/xvfb-$$"
  cp /usr/bin/Xvfb "$bin"
  local n
  for n in $(shuf -i 300-900); do
    [ -e "/tmp/.X11-unix/X$n" ] || [ -e "/tmp/.X$n-lock" ] || break
  done
  "$bin" ":$n" -screen 0 "${XVFB_SIZE}x24" >/dev/null 2>&1 &
  XVFB_PIDS+=($!)
  export DISPLAY=":$n"
  # winit prefers Wayland when it is offered; the client must talk to this X server.
  unset WAYLAND_DISPLAY
  export XDG_SESSION_TYPE=x11 LIBGL_ALWAYS_SOFTWARE=1
  until [ -e "/tmp/.X11-unix/X$n" ]; do sleep 0.2; done
}

# Runs a command in the background on the private display.
# The client keeps its own settings under XDG_CONFIG_HOME, which `--fresh-profile` (the engine's) does not
# cover: a private one keeps the player's window placement and tileset out of reach.
xvfb_launch() {
  mkdir -p "$XVFB_SCRATCH/config-$$"
  XDG_CONFIG_HOME="$XVFB_SCRATCH/config-$$" "$@" >"$XVFB_SCRATCH/client-$$.log" 2>&1 &
  XVFB_PIDS+=($!)
}

xvfb_wait_window() {
  until xdotool search --onlyvisible --name "$1" >/dev/null 2>&1; do sleep 0.5; done
}

# Waits until the screen is not uniformly black: a window maps before its first frame is presented.
xvfb_wait_frame() {
  until [ "$(import -window root -format '%[fx:mean>0.01?1:0]' info: 2>/dev/null)" = 1 ]; do sleep 0.5; done
}

xvfb_focus() {
  xdotool search --name "$1" windowfocus
}

# Saves a screenshot and says whether every process we started is still alive.
xvfb_shot() {
  local pid dead=0
  for pid in "${XVFB_PIDS[@]}"; do kill -0 "$pid" 2>/dev/null || dead=1; done
  [ "$dead" = 0 ] || echo "xvfb_shot: a started process has exited; the picture may be blank" >&2
  import -window root "$1"
}

xvfb_stop() {
  local pid
  for pid in "${XVFB_PIDS[@]}"; do kill "$pid" 2>/dev/null; done
  XVFB_PIDS=()
  rm -f "$XVFB_SCRATCH/xvfb-$$"
}
