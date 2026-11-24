#!/usr/bin/env bash
# Asima Updater (the real GUI, system mode) against asimaos-updaterd on a private bus: the GUI
# verifies and installs a signed update by itself, offers "Reinstall this version" for the
# running version's own file (D21); screenshots show what a person would see.
set -uo pipefail
APP=${1:-build/apps/asimaos-updater/asimaos-updater}
SVC_BIN=${2:-build/services/release/asimaos-updaterd}
OUT=${3:-build/gui-e2e}
T=$(mktemp -d); ROOT=$T/root; mkdir -p "$OUT"
fails=0
check() { if eval "$2"; then echo "  ok    $1"; else echo "  FAIL  $1"; fails=$((fails + 1)); fi; }

ESP=$ROOT/boot
SLOTS=$ROOT/run/initramfs/slots
mkdir -p "$SLOTS" "$ESP/EFI/Asima" "$ROOT/usr/lib" "$ROOT/usr/share/asimaos/keys" "$ROOT/home" "$ROOT/etc/asimaos"
AOSUPD=$(dirname "$SVC_BIN")/aosupd
echo 'IMAGE_VERSION=2026.11.15.1' > "$ROOT/usr/lib/os-release"
# The start menu: 2026.11.15.1 current (started fine), 2026.11.01.2 its backup.
for v in 2026.11.01.2 2026.11.15.1; do
    echo "usr $v" > "$SLOTS/usr_$v.erofs"; echo "v $v" > "$SLOTS/verity_$v.img"; echo "uki $v" > "$ESP/EFI/Asima/asimaos_$v.efi"
    "$AOSUPD" boot promote --esp "$ESP" --version "$v" >/dev/null
    f=$(ls "$ESP/loader/entries" | grep '^asimaos-current'); [[ $f == asimaos-current.conf ]] || mv "$ESP/loader/entries/$f" "$ESP/loader/entries/asimaos-current.conf"
done
"$AOSUPD" keygen "$T/release" && cp "$T/release.pub" "$ROOT/usr/share/asimaos/keys/release.pub"
pack() {  # VERSION OUT
    local v=$1 D=$T/upd-$1 && mkdir -p "$D"
    yes "Asima OS $v" | head -c 30000000 > "$D/usr_$v.erofs"; head -c 3000000 /dev/urandom > "$D/verity_$v.img"; head -c 2000000 /dev/urandom > "$D/asimaos_$v.efi"
    printf -- '- KDE Plasma 6.8.4 and KDE Gear 26.08.3\n- Linux 7.3.6 with security fixes\n- Faster start-up for the Asima OS apps\n' > "$D/NOTES"
    "$AOSUPD" pack --key "$T/release.key" --version "$v" --expires 2099-12-31 \
        --slot "$D/usr_$v.erofs" --slot "$D/verity_$v.img" --uki "$D/asimaos_$v.efi" --notes "$D/NOTES" -o "$2"
}
V=2026.12.01.1
pack "$V" "$T/asimaos-$V.aosupd"
pack 2026.11.15.1 "$T/asimaos-2026.11.15.1.aosupd"

export DBUS_SESSION_BUS_ADDRESS=$(dbus-daemon --session --fork --print-address) ASIMAOS_SERVICES_BUS=session
"$SVC_BIN" --session --root "$ROOT" & SVC=$!
sleep 0.5
export XDG_RUNTIME_DIR=/tmp/xdg-gui && mkdir -p -m 0700 "$XDG_RUNTIME_DIR"
export LIBGL_ALWAYS_SOFTWARE=1 QT_QPA_PLATFORM=xcb DISPLAY=:96
Xvfb :96 -screen 0 1920x1200x24 -nolisten tcp >/dev/null 2>&1 & X=$!
sleep 1
run() { "$APP" --theme light --size 1320x860 "$@" 2>&1 | grep -vE 'propagateSizeHints|XDG_RUNTIME' || true; }

run --page versions --screenshot "$OUT/updater-versions.png"
run "$T/asimaos-2026.11.15.1.aosupd" --screenshot "$OUT/updater-update-reinstall.png" --delay 2500
check "the running version's own file offers a reinstall" '[[ -s $OUT/updater-update-reinstall.png ]]'

run "$T/asimaos-$V.aosupd" --screenshot "$OUT/updater-update-ready.png" --delay 2500
cp "$T/asimaos-$V.aosupd" "$T/damaged.aosupd" && printf 'X' | dd of="$T/damaged.aosupd" bs=1 seek=$(( $(stat -c %s "$T/damaged.aosupd") - 5000 )) conv=notrunc status=none
run "$T/damaged.aosupd" --screenshot "$OUT/updater-update-damaged.png" --delay 2500
check "an update file opens straight into its check" '[[ -s $OUT/updater-update-ready.png ]]'
run --page updates --autotest-update "$T/asimaos-$V.aosupd" --screenshot "$OUT/updater-update-done.png" --delay 6000
check "the GUI installed the update through the service" '[[ -s $ESP/EFI/Asima/asimaos_$V.efi && -s $SLOTS/usr_$V.erofs ]]'
check "…it is current in the start menu (3 tries)" 'grep -qx "version  $V" "$ESP/loader/entries/asimaos-current+3-0.conf"'
check "…the running version is its backup"         'grep -qx "version  2026.11.15.1" "$ESP/loader/entries/asimaos-backup.conf"'
check "…and the service kept two versions"        '[[ ! -e $SLOTS/usr_2026.11.01.2.erofs && -e $SLOTS/usr_2026.11.15.1.erofs ]]'
run --page versions --screenshot "$OUT/updater-versions-after.png"
check "screenshots were taken"                    '[[ -s $OUT/updater-versions.png && -s $OUT/updater-update-done.png && -s $OUT/updater-versions-after.png ]]'

kill $X $SVC 2>/dev/null
rm -rf "$T"
echo "Asima Updater + service: $([[ $fails -eq 0 ]] && echo 'all checks passed' || echo "$fails check(s) failed")"
exit $(( fails > 0 ))
