#!/usr/bin/env bash
# Asima Manager (the real GUI, system mode) against asimaos-managerd on a private bus, with face
# sign-in backed by asimaos-faced (a still picture stands in for the infrared camera).
set -uo pipefail
APP=${1:-build/apps/asimaos-manager/asimaos-manager}
SVC_BIN=${2:-build/services/release/asimaos-managerd}
OUT=${3:-build/gui-e2e}
T=$(mktemp -d); ROOT=$T/root; mkdir -p "$OUT" "$ROOT/etc/asimaos"
fails=0
check() { if eval "$2"; then echo "  ok    $1"; else echo "  FAIL  $1"; fails=$((fails + 1)); fi; }
printf 'ZONE=developer\nKILLSWITCH=yes\n' > "$ROOT/etc/asimaos/shield.conf"

export DBUS_SESSION_BUS_ADDRESS=$(dbus-daemon --session --fork --print-address) ASIMAOS_SERVICES_BUS=session
"$SVC_BIN" --session --root "$ROOT" & SVC=$!
# Face sign-in: one face is enrolled first.
djpeg -grayscale -pnm build/models/lena.jpg > "$T/face.pgm"
build/services/release/asimaos-faced --session --store "$T/faces" --models build/models --picture "$T/face.pgm" & FACE=$!
sleep 1
busctl --user call io.github.asimaos.Face1 /io/github/asimaos/Face1 io.github.asimaos.Face1 Enroll s "Your face" >/dev/null
for _ in $(seq 300); do [[ -f $T/faces/$(id -u).json ]] && break; sleep 0.1; done
export XDG_RUNTIME_DIR=/tmp/xdg-md && mkdir -p -m 0700 "$XDG_RUNTIME_DIR"
export LIBGL_ALWAYS_SOFTWARE=1 QT_QPA_PLATFORM=xcb DISPLAY=:95
Xvfb :95 -screen 0 1920x1200x24 -nolisten tcp >/dev/null 2>&1 & X=$!
sleep 1
run() { "$APP" --size 1320x860 "$@" 2>&1 | grep -vE 'propagateSizeHints|XDG_RUNTIME' || true; }
run --theme dark --page shield --screenshot "$OUT/manager-shield.png"
check "screenshot of the service-backed Shield page" '[[ -s $OUT/manager-shield.png ]]'
run --theme light --page biometrics --screenshot "$OUT/manager-face.png"
check "screenshot of face sign-in backed by asimaos-faced" '[[ -s $OUT/manager-face.png ]]'
run --theme light --page home --screenshot "$OUT/manager-home.png"
check "screenshot of the home page on this computer" '[[ -s $OUT/manager-home.png ]]'
kill $X $SVC $FACE 2>/dev/null
rm -rf "$T"
echo "Asima Manager + services: $([[ $fails -eq 0 ]] && echo 'all checks passed' || echo "$fails check(s) failed")"
exit $(( fails > 0 ))
