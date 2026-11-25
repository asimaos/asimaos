#!/usr/bin/env bash
# asimaos-updaterd end to end, without a real Asima OS: a private D-Bus bus, a scratch system tree,
# a throwaway signing key and real .aosupd update files made with aosupd. Covers updates and the
# start menu (current / backup / failed), the snapshot before an update, the parts applied after a
# good start, resuming and reinstalling (D21), and the aosupd tool Asima Rescue uses.
set -uo pipefail
BIN=${1:-build/services/release/asimaos-updaterd}
AOSUPD=$(dirname "$BIN")/aosupd
T=$(mktemp -d)
ROOT=$T/root
ESP=$ROOT/boot
SLOTS=$ROOT/run/initramfs/slots
fails=0
check() { if eval "$2"; then echo "  ok    $1"; else echo "  FAIL  $1"; fails=$((fails + 1)); fi; }
call() { busctl --user --json=short call io.github.asimaos.Updater1 /io/github/asimaos/Updater1 io.github.asimaos.Updater1 "$@"; }
entry() { ls "$ESP/loader/entries" | grep "^asimaos-$1" ; }
role() { local f; f=$(entry "$1"); [[ -n $f ]] && sed -n 's/^version *//p' "$ESP/loader/entries/$f"; }
waitfor() { for _ in $(seq 150); do eval "$1" && return 0; sleep 0.1; done; return 1; }
running() { echo "IMAGE_VERSION=$1" > "$ROOT/usr/lib/os-release"; }
bless() { local f; f=$(entry "$1"); [[ $f == "asimaos-$1.conf" ]] || mv "$ESP/loader/entries/$f" "$ESP/loader/entries/asimaos-$1.conf"; }

# ── Asima OS 2026.11.15.1 (current) with 2026.11.01.2 as its backup
mkdir -p "$ROOT/usr/lib" "$SLOTS" "$ESP/EFI/Asima" "$ESP/EFI/BOOT" "$ROOT/usr/share/asimaos/keys" "$ROOT/run/initramfs/data/flags" \
         "$ROOT/etc/asimaos" "$ROOT/pool/@home/asha" "$ROOT/pool/@data" "$ROOT/pool/@var" "$ROOT/pool/@root" "$ROOT/home/asha"
echo "notes of asha" > "$ROOT/pool/@home/asha/notes.txt"
for i in 1 2 3; do head -c 4000 /dev/urandom > "$ROOT/home/asha/photo$i.jpg"; done
running 2026.11.15.1
# Secure Boot the Asima OS way: shim is BOOTX64.EFI, systemd-boot is grubx64.efi next to MokManager.
echo "old shim" > "$ESP/EFI/BOOT/BOOTX64.EFI"
echo "old mm" > "$ESP/EFI/BOOT/mmx64.efi"
echo "old loader" > "$ESP/EFI/BOOT/grubx64.efi"
for v in 2026.11.01.2 2026.11.15.1; do
    echo "usr $v" > "$SLOTS/usr_$v.erofs"
    echo "verity $v" > "$SLOTS/verity_$v.img"
    echo "uki $v" > "$ESP/EFI/Asima/asimaos_$v.efi"
done
"$AOSUPD" boot promote --esp "$ESP" --version 2026.11.01.2 >/dev/null && bless current
"$AOSUPD" boot promote --esp "$ESP" --version 2026.11.15.1 >/dev/null && bless current
"$AOSUPD" keygen "$T/release" && cp "$T/release.pub" "$ROOT/usr/share/asimaos/keys/release.pub"
# Its first start notes the version its settings are in; a fresh system runs no steps.
"$BIN" --root "$ROOT" --run-migrations >/dev/null
"$AOSUPD" keygen "$T/other"

# make_update VERSION OUT [KEY] [extra pack options…]
make_update() {
    local v=$1 out=$2 key=${3:-$T/release.key} d; shift 3 2>/dev/null || shift $#
    d=$(mktemp -d)
    yes "Asima OS $v system files" | head -c 3000000 > "$d/usr_$v.erofs"
    head -c 300000 /dev/urandom > "$d/verity_$v.img"
    head -c 200000 /dev/urandom > "$d/asimaos_$v.efi"
    printf -- '- Plasma 6.8.4\n- Faster start-up\n' > "$d/NOTES"
    echo "new loader $v" > "$d/systemd-bootx64.efi"
    echo "rescue $v" > "$d/asimaos-rescue.efi"
    echo "shim $v" > "$d/shimx64.efi"
    echo "mm $v" > "$d/mmx64.efi"
    # Migration steps (D52): one newer than the settings of the computer below, one older.
    printf 'echo migrated >> %s\n' "$T/migrated" > "$d/2026.11.20.1-example.sh"
    printf 'echo old >> %s\n' "$T/migrated-old" > "$d/2026.11.10.1-older.sh"
    echo 2026.11.01.1 > "$d/steps-from"
    "$AOSUPD" pack --key "$key" --version "$v" --expires 2099-12-31 \
        --slot "$d/usr_$v.erofs" --slot "$d/verity_$v.img" --uki "$d/asimaos_$v.efi" --notes "$d/NOTES" \
        --bootloader "$d/systemd-bootx64.efi" --shim "$d/shimx64.efi" --shim "$d/mmx64.efi" \
        --rescue "$d/asimaos-rescue.efi" --migration "$d/2026.11.20.1-example.sh" \
        --migration "$d/2026.11.10.1-older.sh" --migration "$d/steps-from" "$@" -o "$out"
    rm -rf "$d"
}
make_update 2026.12.01.1 "$T/good.aosupd"
make_update 2026.12.01.1 "$T/foreign.aosupd" "$T/other.key"
make_update 2026.11.01.2 "$T/older.aosupd"
make_update 2026.12.01.1 "$T/later.aosupd" "$T/release.key" --min-from 2026.11.30.1
# The next update also carries a part of a kind this updater doesn't know (a later version's).
echo "a part only later versions know" > "$T/example.bin"
make_update 2026.12.15.1 "$T/next.aosupd" "$T/release.key" --part "example=$T/example.bin"
head -c -1000 "$T/good.aosupd" > "$T/cut.aosupd"
cp "$T/good.aosupd" "$T/flipped.aosupd" && printf 'X' | dd of="$T/flipped.aosupd" bs=1 seek=$(( $(stat -c %s "$T/good.aosupd") - 5000 )) conv=notrunc status=none

# ── private bus + service
export DBUS_SESSION_BUS_ADDRESS=$(dbus-daemon --session --fork --print-address)
(umask 077; exec "$BIN" --session --root "$ROOT") & SVC=$!   # UMask=0077, like its systemd unit
for _ in $(seq 50); do busctl --user status io.github.asimaos.Updater1 >/dev/null 2>&1 && break; sleep 0.1; done

S=$(call Status)
check "Status answers without a password"        '[[ $S == *"\\\"version\\\":\\\"2026.11.15.1\\\""* ]]'
check "Status shows current and backup"          '[[ $S == *"\\\"role\\\":\\\"current\\\""* && $S == *"\\\"role\\\":\\\"backup\\\""* && $S != *rofiles* ]]'

V=$(call VerifyUpdate h 3 3<"$T/good.aosupd")
check "a genuine newer update verifies"          '[[ $V == *"\\\"ok\\\":true"* && $V == *"\\\"signature\\\":true"* ]]'
check "…with its release notes"                  '[[ $V == *"Plasma 6.8.4"* ]]'
V=$(call VerifyUpdate h 3 3<"$T/foreign.aosupd")
check "another key's update is refused"          '[[ $V == *"\\\"ok\\\":false"* && $V == *"signed by Asima OS"* && $V == *"\\\"refused\\\":\\\"not-signed\\\""* ]]'
V=$(call VerifyUpdate h 3 3<"$T/older.aosupd")
check "an older version is refused"              '[[ $V == *"\\\"ok\\\":false"* && $V == *"not newer"* && $V == *"\\\"failed\\\":[\\\"newer\\\"]"* ]]'
V=$(call VerifyUpdate h 3 3<"$T/cut.aosupd")
check "a cut-off download is refused at once"    '[[ $V == *"\\\"ok\\\":false"* && $V == *"incomplete"* && $V == *"\\\"refused\\\":\\\"damaged\\\""* ]]'
V=$(call VerifyUpdate h 3 3<"$T/later.aosupd")
check "MIN_FROM is enforced"                     '[[ $V == *"\\\"ok\\\":false"* && $V == *"installs on Asima OS 2026.11.30.1 or newer"* && $V == *"\\\"min-from\\\""* ]]'
V=$(call VerifyUpdate h 3 3<"$T/flipped.aosupd")
check "a damaged part is caught before writing"  '[[ $V == *"\\\"ok\\\":false"* && $V == *"damaged"* && $V == *"\\\"signature\\\":true"* && $V == *"\\\"refused\\\":\\\"damaged\\\""* ]]'

call ApplyUpdate h 3 3<"$T/flipped.aosupd" >/dev/null 2>&1
check "a damaged update changes nothing"         '[[ ! -e $SLOTS/usr_2026.12.01.1.erofs && $(role current) == 2026.11.15.1 ]]'

# ── an update: the new version is current on trial, the running one its backup
J=$(call ApplyUpdate h 3 3<"$T/good.aosupd")
check "ApplyUpdate starts a job"                 '[[ $J == *update-* ]]'
waitfor '[[ $(role current) == 2026.12.01.1 && ! -e $SLOTS/usr_2026.11.01.2.erofs ]]'
check "the image and verity data are in the slots" '[[ $(head -n1 $SLOTS/usr_2026.12.01.1.erofs) == "Asima OS 2026.12.01.1 system files" && -s $SLOTS/verity_2026.12.01.1.img ]]'
check "…and a copy of its boot image (for repairs)" '[[ $(sha256sum < $SLOTS/boot/asimaos_2026.12.01.1.efi) == $(sha256sum < $ESP/EFI/Asima/asimaos_2026.12.01.1.efi) ]]'
check "start menu: new = current with 3 tries"   '[[ $(entry current) == asimaos-current+3-0.conf && -s $ESP/EFI/Asima/asimaos_2026.12.01.1.efi ]]'
check "…the running version = backup, on top second" '[[ $(role backup) == 2026.11.15.1 ]] && grep -q "Asima OS 2026.11.15.1 (backup)" $ESP/loader/entries/asimaos-backup.conf'
check "…the older backup is gone with its files" '[[ ! -e $SLOTS/usr_2026.11.01.2.erofs && ! -e $ESP/EFI/Asima/asimaos_2026.11.01.2.efi ]]'
check "a snapshot was taken before the update"   '[[ $(cat $ROOT/pool/@snapshots/update/2026.12.01.1/home/asha/notes.txt) == "notes of asha" && -d $ROOT/pool/@snapshots/update/2026.12.01.1/data ]]'
check "boot loader, shim and Rescue wait (staged)" '[[ -f $SLOTS/staged/2026.12.01.1/systemd-bootx64.efi && -f $SLOTS/staged/2026.12.01.1/shimx64.efi && $(<$ESP/EFI/BOOT/grubx64.efi) == "old loader" && $(<$ESP/EFI/BOOT/BOOTX64.EFI) == "old shim" ]]'

# ── it starts and is marked good: the staged parts are applied once
running 2026.12.01.1
"$BIN" --root "$ROOT" --finish-update >/dev/null
check "not before the new version is marked good" '[[ $(<$ESP/EFI/BOOT/grubx64.efi) == "old loader" && $(<$ESP/EFI/BOOT/BOOTX64.EFI) == "old shim" ]]'
bless current
"$BIN" --root "$ROOT" --finish-update >/dev/null
check "after a good start: new boot loader"      '[[ $(<$ESP/EFI/BOOT/grubx64.efi) == "new loader 2026.12.01.1" ]]'
check "…and the new shim with its MokManager"    '[[ $(<$ESP/EFI/BOOT/BOOTX64.EFI) == "shim 2026.12.01.1" && $(<$ESP/EFI/BOOT/mmx64.efi) == "mm 2026.12.01.1" && ! -e $SLOTS/staged/2026.12.01.1/shimx64.efi ]]'
check "…and the new Rescue, in the start menu"   '[[ $(<$ESP/EFI/Asima/asimaos-rescue.efi) == "rescue 2026.12.01.1" ]] && grep -q "Asima Rescue" $ESP/loader/entries/asimaos-rescue.conf'
"$BIN" --root "$ROOT" --run-migrations >/dev/null; rm -f "$T/migrated"; "$BIN" --root "$ROOT" --run-migrations >/dev/null
check "migrations run once"                      '[[ ! -e $T/migrated && -f $ROOT/var/lib/asimaos/migrations/2026.12.01.1 ]]'
check "…only those newer than the settings' version" '[[ ! -e $T/migrated-old && $(<$ROOT/var/lib/asimaos/state-version) == 2026.12.01.1 && ! -e $ROOT/var/lib/asimaos/settings-behind ]]'
# Settings older than the steps reach (more than a year skipped, through Rescue or the repair).
echo 2026.10.01.1 > "$ROOT/var/lib/asimaos/state-version"; rm -rf "$ROOT/var/lib/asimaos/migrations/steps"
"$BIN" --root "$ROOT" --run-migrations >/dev/null
check "…older settings: every step runs, and Maintenance is told" '[[ -e $T/migrated && -e $T/migrated-old && $(<$ROOT/var/lib/asimaos/settings-behind) == "2026.10.01.1 2026.11.01.1" ]]'
rm -f "$T/migrated" "$T/migrated-old" "$ROOT/var/lib/asimaos/settings-behind"

# ── going back by hand swaps current and backup; going forward again swaps back
call SetDefaultVersion sb 2026.11.15.1 false >/dev/null
check "back to the backup: roles swap"           '[[ $(role current) == 2026.11.15.1 && $(role backup) == 2026.12.01.1 ]]'
call SetDefaultVersion sb 2026.12.01.1 false >/dev/null
check "…and forward again"                       '[[ $(role current) == 2026.12.01.1 && $(role backup) == 2026.11.15.1 ]]'

# ── a version that keeps failing: the start menu falls back, then the roles settle
f=$(entry current); mv "$ESP/loader/entries/$f" "$ESP/loader/entries/asimaos-current+0-3.conf"
running 2026.11.15.1
mkdir -p "$ROOT/var/lib/asimaos/migrations" && echo 1 > "$ROOT/var/lib/asimaos/migrations/2026.12.01.1"   # it changed the system
"$BIN" --root "$ROOT" --finish-update >/dev/null
check "after a fallback the working version is current" '[[ $(role current) == 2026.11.15.1 && $(role failed) == 2026.12.01.1 && -z $(role backup) ]]'
check "…and its system changes go back at the next start" '[[ $(<$ROOT/run/initramfs/data/flags/rollback) == 2026.12.01.1 ]]'
rm -f "$ROOT/run/initramfs/data/flags/rollback" "$ROOT/var/lib/asimaos/migrations/2026.12.01.1"
call SetDefaultVersion sb 2026.12.01.1 false >/dev/null
check "a failed update can get a fresh try"      '[[ $(role current) == 2026.12.01.1 && $(entry current) == asimaos-current+3-0.conf && $(role backup) == 2026.11.15.1 ]]'
bless current
running 2026.12.01.1

# ── an interrupted update continues where it stopped (D21)
cp "$T/next.aosupd" "$T/n.aosupd"; d=$(mktemp -d)
"$AOSUPD" install --key "$ROOT/usr/share/asimaos/keys/release.pub" --slots "$d/slots" --esp "$d/esp" "$T/next.aosupd" >/dev/null
cp "$d/slots/usr_2026.12.15.1.erofs" "$SLOTS/"; rm -rf "$d"
echo junk > "$SLOTS/.verity_2026.12.15.1.img.partial"
inode=$(stat -c %i "$SLOTS/usr_2026.12.15.1.erofs")
call ApplyUpdate h 3 3<"$T/next.aosupd" >/dev/null
waitfor '[[ $(role current) == 2026.12.15.1 ]]'
check "a resumed update keeps the finished part" '[[ $(stat -c %i $SLOTS/usr_2026.12.15.1.erofs) == $inode && -s $SLOTS/verity_2026.12.15.1.img && ! -e $SLOTS/.verity_2026.12.15.1.img.partial ]]'
check "…and finishes the rest"                   '[[ $(role backup) == 2026.12.01.1 && ! -e $SLOTS/usr_2026.11.15.1.erofs ]]'
check "a part of a new kind waits for the new version" '[[ $(<$SLOTS/staged/2026.12.15.1/parts/example/example.bin) == "a part only later versions know" ]]'
bless current
running 2026.12.15.1

# ── "Reinstall this version" writes a fresh copy of the running version only (D21)
V=$(call VerifyUpdate h 3 3<"$T/next.aosupd")
check "the running version's own file is offered for reinstall" '[[ $V == *"\\\"sameVersion\\\":true"* && $V == *"\\\"failed\\\":[\\\"newer\\\"]"* ]]'
call ReinstallVersion h 3 3<"$T/good.aosupd" >/dev/null 2>&1
check "reinstalling another version is refused"  '[[ $? -ne 0 && $(role current) == 2026.12.15.1 ]]'
echo damaged > "$SLOTS/usr_2026.12.15.1.erofs"
J=$(call ReinstallVersion h 3 3<"$T/next.aosupd")
waitfor '[[ $(head -n1 $SLOTS/usr_2026.12.15.1.erofs) == "Asima OS 2026.12.15.1 system files" ]]'
check "a reinstall repairs the running version"  '[[ $J == *reinstall-* && $(head -n1 $SLOTS/usr_2026.12.15.1.erofs) == "Asima OS 2026.12.15.1 system files" ]]'
waitfor '[[ $(entry current) == asimaos-current+3-0.conf ]]'
check "…with a fresh trial, the backup untouched" '[[ $(entry current) == asimaos-current+3-0.conf && $(role backup) == 2026.12.01.1 ]]'

# ── the snapshots taken before updates (Asima Maintenance lists them)
check "snapshots before updates: the last 2 updates" '[[ $(ls $ROOT/pool/@snapshots/update | tr "\n" " ") == "2026.12.01.1 2026.12.15.1 " ]]'

# ── the command-line installer Asima Rescue uses: same checks, JSON events, the start menu
mkdir -p "$T/rescue/slots" "$T/rescue/esp" "$T/rescue2/slots" "$T/rescue2/esp"
PUB=$ROOT/usr/share/asimaos/keys/release.pub
E=$("$AOSUPD" install --json --key "$PUB" --slots "$T/rescue/slots" --esp "$T/rescue/esp" "$T/good.aosupd")
check "aosupd install (Rescue) reports its steps" '[[ $E == *"\"id\":\"verify\",\"state\":\"done\""* && $(tail -n1 <<<"$E") == *"\"event\":\"done\""*"\"version\":\"2026.12.01.1\""* ]]'
check "…writes what Asima Updater writes, and makes it current" '[[ $(head -n1 $T/rescue/slots/usr_2026.12.01.1.erofs) == "Asima OS 2026.12.01.1 system files" && -s $T/rescue/esp/EFI/Asima/asimaos_2026.12.01.1.efi && -f $T/rescue/esp/loader/entries/asimaos-current+3-0.conf && -f $T/rescue/slots/staged/2026.12.01.1/asimaos-rescue.efi ]]'
E=$("$AOSUPD" install --json --key "$PUB" --slots "$T/rescue2/slots" --esp "$T/rescue2/esp" "$T/flipped.aosupd"); rc=$?
check "…a damaged file: one error, nothing written" '[[ $rc -eq 1 && $(grep -c "\"event\":\"error\"" <<<"$E") -eq 1 && -z $(find $T/rescue2 -mindepth 2) ]]'
"$AOSUPD" install --json --key "$PUB" --slots "$T/rescue2/slots" --esp "$T/rescue2/esp" --typo x "$T/good.aosupd" >/dev/null; rc=$?
check "…and a mistyped option is not ignored"    '[[ $rc -eq 1 && -z $(find $T/rescue2 -mindepth 2) ]]'

kill $SVC 2>/dev/null
wait $SVC 2>/dev/null
# Idle exit: with nothing to do the service ends, so the ESP it mounted is released;
# a call starts it again (here by hand: the private test bus has no activation).
ASIMAOS_IDLE_EXIT=2 "$BIN" --session --root "$ROOT" & IDLE=$!
for _ in $(seq 50); do busctl --user status io.github.asimaos.Updater1 >/dev/null 2>&1 && break; sleep 0.1; done
call Status >/dev/null
sleep 1.2; call Status >/dev/null   # a call keeps it running
alive=no; kill -0 $IDLE 2>/dev/null && alive=yes
for _ in $(seq 80); do kill -0 $IDLE 2>/dev/null || break; sleep 0.1; done
check "idle: kept running while called, then exits by itself" '[[ $alive == yes ]] && ! kill -0 $IDLE 2>/dev/null'
kill $IDLE 2>/dev/null
rm -rf "$T"
echo "asimaos-updaterd: $([[ $fails -eq 0 ]] && echo 'all checks passed' || echo "$fails check(s) failed")"
exit $(( fails > 0 ))
