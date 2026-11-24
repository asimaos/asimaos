#!/usr/bin/env bash
# Asima Gaming (the real GUI, system mode) against asimaos-gamingd on a private bus with a scratch
# sysfs: the pages render with what this computer and the service report.
set -uo pipefail
APP=${1:-build/apps/asimaos-gaming/asimaos-gaming}
SVC_BIN=${2:-build/services/release/asimaos-gamingd}
OUT=${3:-build/gui-e2e}
T=$(mktemp -d); ROOT=$T/root; mkdir -p "$OUT"
fails=0
check() { if eval "$2"; then echo "  ok    $1"; else echo "  FAIL  $1"; fails=$((fails + 1)); fi; }
for a in ppt_pl1_spl ppt_pl2_sppt ppt_fppt; do
    d=$ROOT/sys/class/firmware-attributes/asus-armoury/attributes/$a && mkdir -p "$d"
    echo 17 > "$d/current_value"; echo 7 > "$d/min_value"; echo 30 > "$d/max_value"
done
LED=$ROOT/sys/class/leds/input5:rgb:indicator && mkdir -p "$LED/device"
echo "DualSense Wireless Controller" > "$LED/device/name"
echo "red green blue" > "$LED/multi_index"; echo 255 > "$LED/max_brightness"; echo "0 0 255" > "$LED/multi_intensity"; echo 255 > "$LED/brightness"

export DBUS_SESSION_BUS_ADDRESS=$(dbus-daemon --session --fork --print-address) ASIMAOS_SERVICES_BUS=session
"$SVC_BIN" --session --root "$ROOT" & SVC=$!
sleep 0.5
export XDG_RUNTIME_DIR=/tmp/xdg-gm && mkdir -p -m 0700 "$XDG_RUNTIME_DIR"
export LIBGL_ALWAYS_SOFTWARE=1 QT_QPA_PLATFORM=xcb DISPLAY=:93
Xvfb :93 -screen 0 1920x1200x24 -nolisten tcp >/dev/null 2>&1 & X=$!
sleep 1
run() { "$APP" --theme dark --size 1320x860 "$@" 2>&1 | grep -vE 'propagateSizeHints|XDG_RUNTIME' || true; }
run --page gaming --screenshot "$OUT/gaming-gaming.png"
check "screenshot of the gaming page on this computer" '[[ -s $OUT/gaming-gaming.png ]]'
run --page controllers --screenshot "$OUT/gaming-controllers.png"
check "screenshot of the controllers page"             '[[ -s $OUT/gaming-controllers.png ]]'
run --page lighting --screenshot "$OUT/gaming-lighting.png"
check "screenshot of the lighting page (the service's light)" '[[ -s $OUT/gaming-lighting.png ]]'
kill $X $SVC 2>/dev/null
rm -rf "$T"
echo "Asima Gaming + service: $([[ $fails -eq 0 ]] && echo 'all checks passed' || echo "$fails check(s) failed")"
exit $(( fails > 0 ))
