#!/usr/bin/env bash
# asimaos-gamingd end to end on a private D-Bus bus with a scratch sysfs (no real hardware touched):
# a handheld's power limit, the CPU scheduler request and RGB lighting.
set -uo pipefail
BIN=${1:-build/services/release/asimaos-gamingd}
T=$(mktemp -d)
ROOT=$T/root
fails=0
check() { if eval "$2"; then echo "  ok    $1"; else echo "  FAIL  $1"; fails=$((fails + 1)); fi; }
call() { busctl --user --json=short call io.github.asimaos.Gaming1 /io/github/asimaos/Gaming1 io.github.asimaos.Gaming1 "$@"; }

# a handheld with ASUS power limits
for a in ppt_pl1_spl ppt_pl2_sppt ppt_fppt; do
    d=$ROOT/sys/class/firmware-attributes/asus-armoury/attributes/$a && mkdir -p "$d"
    echo 17 > "$d/current_value"; echo 7 > "$d/min_value"; echo 30 > "$d/max_value"
done
# a controller's RGB light bar, and a single-colour LED that is not ours
LED=$ROOT/sys/class/leds/input5:rgb:indicator && mkdir -p "$LED/device"
echo "DualSense Wireless Controller" > "$LED/device/name"
echo "red green blue" > "$LED/multi_index"; echo 255 > "$LED/max_brightness"; echo "0 0 255" > "$LED/multi_intensity"; echo 255 > "$LED/brightness"
mkdir -p "$ROOT/sys/class/leds/input3::capslock"

export DBUS_SESSION_BUS_ADDRESS=$(dbus-daemon --session --fork --print-address)
"$BIN" --session --root "$ROOT" & SVC=$!
for _ in $(seq 50); do busctl --user status io.github.asimaos.Gaming1 >/dev/null 2>&1 && break; sleep 0.1; done

S=$(call Status)
check "Status answers without a password"        '[[ $S == *"\\\"max\\\":30"* && $S == *"\\\"method\\\":\\\"asus\\\""* ]]'
call Set ssv handheld tdp i 25 >/dev/null
check "power limit 25 W on all three ASUS limits" '[[ $(for f in $ROOT/sys/class/firmware-attributes/asus-armoury/attributes/*/current_value; do printf "%s," "$(<$f)"; done) == 25,25,25, ]]'
call Set ssv handheld tdp i 40 >/dev/null 2>&1
check "a limit above the device's maximum is refused" '[[ $? -ne 0 && $(<$ROOT/sys/class/firmware-attributes/asus-armoury/attributes/ppt_fppt/current_value) == 25 ]]'
E=$(call Set ssv gaming scheduler s lavd 2>&1)
check "without scx_loader the scheduler says so"  '[[ $E == *"scx_loader"* ]]'
call Set ssv gaming scheduler s fastest >/dev/null 2>&1
check "an unknown scheduler is refused"           '[[ $? -ne 0 ]]'
call Set ssv power chargeLimit i 80 >/dev/null 2>&1
check "device settings are not this service's"    '[[ $? -ne 0 ]]'

check "Status lists the one RGB light, by name"   '[[ $S == *"Light bar"* && $S == *"DualSense Wireless Controller"* && $S != *capslock* ]]'
call Set ssv lighting color s "#2F80ED:50" >/dev/null
check "light set to Asima blue at 50 %"          '[[ $(<$LED/multi_intensity) == "47 128 237" && $(<$LED/brightness) == 127 ]]'
check "…and the colour is kept in /etc"           'grep -qx "COLOR=#2F80ED" $ROOT/etc/asimaos/lighting.conf && grep -qx BRIGHTNESS=50 $ROOT/etc/asimaos/lighting.conf'
call Set ssv lighting color s "blue:50" >/dev/null 2>&1
check "a colour that isn't #RRGGBB is refused"   '[[ $? -ne 0 && $(<$LED/multi_intensity) == "47 128 237" ]]'
echo "0 0 255" > "$LED/multi_intensity"
"$BIN" --root "$ROOT" --apply-lighting "$LED" >/dev/null
check "a reconnected controller gets the colour back (udev)" '[[ $(<$LED/multi_intensity) == "47 128 237" ]]'

kill $SVC 2>/dev/null
rm -rf "$T"
echo "asimaos-gamingd: $([[ $fails -eq 0 ]] && echo 'all checks passed' || echo "$fails check(s) failed")"
exit $(( fails > 0 ))
