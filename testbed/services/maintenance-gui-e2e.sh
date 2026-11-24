#!/usr/bin/env bash
# Asima Maintenance (the real GUI, system mode) against asimaos-maintenanced on a private bus: the
# GUI shows the service's status, takes a snapshot through it, and the pages render.
set -uo pipefail
APP=${1:-build/apps/asimaos-maintenance/asimaos-maintenance}
SVC_BIN=${2:-build/services/release/asimaos-maintenanced}
OUT=${3:-build/gui-e2e}
T=$(mktemp -d); ROOT=$T/root
mkdir -p "$OUT" "$ROOT/usr/lib" "$ROOT/etc/asimaos" "$ROOT/run/initramfs/data/flags" "$ROOT/pool/@home/asha" "$ROOT/home"
echo 'IMAGE_VERSION=2026.11.15.1' > "$ROOT/usr/lib/os-release"
echo "notes of asha" > "$ROOT/pool/@home/asha/notes.txt"
fails=0
check() { if eval "$2"; then echo "  ok    $1"; else echo "  FAIL  $1"; fails=$((fails + 1)); fi; }

export DBUS_SESSION_BUS_ADDRESS=$(dbus-daemon --session --fork --print-address) ASIMAOS_SERVICES_BUS=session
"$SVC_BIN" --session --root "$ROOT" & SVC=$!
sleep 0.5
busctl --user call io.github.asimaos.Maintenance1 /io/github/asimaos/Maintenance1 io.github.asimaos.Maintenance1 SnapshotNow >/dev/null
export XDG_RUNTIME_DIR=/tmp/xdg-mt && mkdir -p -m 0700 "$XDG_RUNTIME_DIR"
export LIBGL_ALWAYS_SOFTWARE=1 QT_QPA_PLATFORM=xcb DISPLAY=:94
Xvfb :94 -screen 0 1920x1200x24 -nolisten tcp >/dev/null 2>&1 & X=$!
sleep 1
run() { "$APP" --theme light --size 1320x860 "$@" 2>&1 | grep -vE 'propagateSizeHints|XDG_RUNTIME' || true; }
run --page overview --screenshot "$OUT/maintenance-overview.png"
check "screenshot of the overview with the service's status" '[[ -s $OUT/maintenance-overview.png ]]'
run --page backups --screenshot "$OUT/maintenance-backups.png"
check "screenshot of the snapshot the service took"           '[[ -s $OUT/maintenance-backups.png && -d $(ls -d $ROOT/pool/@snapshots/home/* | head -n1) ]]'
run --page security --screenshot "$OUT/maintenance-security.png"
check "screenshot of the security page"                       '[[ -s $OUT/maintenance-security.png ]]'
kill $X $SVC 2>/dev/null
rm -rf "$T"
echo "Asima Maintenance + service: $([[ $fails -eq 0 ]] && echo 'all checks passed' || echo "$fails check(s) failed")"
exit $(( fails > 0 ))
