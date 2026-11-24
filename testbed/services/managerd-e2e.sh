#!/usr/bin/env bash
# asimaos-managerd end to end on a private D-Bus bus with a scratch sysfs (no real hardware touched):
# battery charge limit, where face sign-in may be used, and Asima Shield.
set -uo pipefail
BIN=${1:-build/services/release/asimaos-managerd}
T=$(mktemp -d)
ROOT=$T/root
fails=0
check() { if eval "$2"; then echo "  ok    $1"; else echo "  FAIL  $1"; fails=$((fails + 1)); fi; }
call() { busctl --user --json=short call io.github.asimaos.Manager1 /io/github/asimaos/Manager1 io.github.asimaos.Manager1 "$@"; }

# a laptop with an RGB light (Asima Gaming's, not this service's) and a battery with a charge limit
LED=$ROOT/sys/class/leds/input5:rgb:indicator && mkdir -p "$LED"
echo "red green blue" > "$LED/multi_index"; echo 255 > "$LED/max_brightness"; echo "0 0 0" > "$LED/multi_intensity"; echo 0 > "$LED/brightness"
BAT=$ROOT/sys/class/power_supply/BAT0 && mkdir -p "$BAT"
echo Battery > "$BAT/type"; echo 100 > "$BAT/charge_control_end_threshold"

export DBUS_SESSION_BUS_ADDRESS=$(dbus-daemon --session --fork --print-address)
"$BIN" --session --root "$ROOT" & SVC=$!
for _ in $(seq 50); do busctl --user status io.github.asimaos.Manager1 >/dev/null 2>&1 && break; sleep 0.1; done

S=$(call Status)
check "Status answers without a password"        '[[ $S == *"zone\\\":\\\"home"* && $S == *chargeLimit* && $S == *"useLogin\\\":true"* ]]'
call Set ssv power chargeLimit i 80 >/dev/null
check "charge limit set to 80 %"                  '[[ $(<$BAT/charge_control_end_threshold) == 80 ]]'
call Set ssv power chargeLimit i 20 >/dev/null 2>&1
check "an unsafe charge limit is refused"         '[[ $? -ne 0 && $(<$BAT/charge_control_end_threshold) == 80 ]]'
call Set ssv display lighting s "#2F80ED:50" >/dev/null 2>&1
check "lighting is Asima Gaming's, not this service's" '[[ $? -ne 0 && $(<$LED/multi_intensity) == "0 0 0" ]]'
call Set ssv handheld tdp i 25 >/dev/null 2>&1
check "gaming settings are not this service's"    '[[ $? -ne 0 ]]'
call SetShield sv zone s developer >/dev/null
check "firewall zone saved through /etc"          'grep -qx ZONE=developer $ROOT/etc/asimaos/shield.conf'
call SetShield sv zone s wide-open >/dev/null 2>&1
check "an unknown zone is refused"                '[[ $? -ne 0 ]] && grep -qx ZONE=developer $ROOT/etc/asimaos/shield.conf'
call Set ssv biometrics useLogin b false >/dev/null
check "face sign-in can be switched off for login" 'grep -qx LOGIN=no $ROOT/etc/asimaos/face.conf'
S=$(call Status)
check "…and Status reports it"                    '[[ $S == *"useLogin\\\":false"* && $S == *"useUnlock\\\":true"* ]]'
check "a normal webcam is off for faces at first"  '[[ $S == *"useWebcam\\\":false"* ]]'
call Set ssv biometrics useWebcam b true >/dev/null
S=$(call Status)
check "…and can be switched on (WEBCAM=yes)"     'grep -qx WEBCAM=yes $ROOT/etc/asimaos/face.conf && [[ $S == *"useWebcam\\\":true"* ]]'

call AndroidSetup >/dev/null 2>&1
check "Android is Asima Dev's, not this service's" '[[ $? -ne 0 && ! -e $ROOT/var/lib/waydroid ]]'

kill $SVC 2>/dev/null
rm -rf "$T"
echo "asimaos-managerd: $([[ $fails -eq 0 ]] && echo 'all checks passed' || echo "$fails check(s) failed")"
exit $(( fails > 0 ))
