#!/usr/bin/env bash
# asimaos-maintenanced end to end, without a real Asima OS: a private D-Bus bus and a scratch system
# tree. Covers the status and the health check, snapshots, backups (when restic is available),
# restoring /etc files, resetting the settings and the factory reset staging.
set -uo pipefail
BIN=${1:-build/services/release/asimaos-maintenanced}
T=$(mktemp -d)
ROOT=$T/root
fails=0
check() { if eval "$2"; then echo "  ok    $1"; else echo "  FAIL  $1"; fails=$((fails + 1)); fi; }
call() { busctl --user --json=short call io.github.asimaos.Maintenance1 /io/github/asimaos/Maintenance1 io.github.asimaos.Maintenance1 "$@"; }
waitfor() { for _ in $(seq 150); do eval "$1" && return 0; sleep 0.1; done; return 1; }

# Asima OS 2026.12.15.1 with a set of snapshots its update took (asimaos-updaterd does that)
mkdir -p "$ROOT/usr/lib" "$ROOT/etc/asimaos" "$ROOT/run/initramfs/data/flags" "$ROOT/run/initramfs/data/overlay/etc/upper/ssh" \
         "$ROOT/pool/@home/asha" "$ROOT/pool/@data" "$ROOT/pool/@var" "$ROOT/pool/@root" "$ROOT/home/asha" \
         "$ROOT/pool/@snapshots/update/2026.12.15.1/home"
echo "IMAGE_VERSION=2026.12.15.1" > "$ROOT/usr/lib/os-release"
echo "notes of asha" > "$ROOT/pool/@home/asha/notes.txt"
for i in 1 2 3; do head -c 4000 /dev/urandom > "$ROOT/home/asha/photo$i.jpg"; done
echo myhost > "$ROOT/run/initramfs/data/overlay/etc/upper/hostname"

export DBUS_SESSION_BUS_ADDRESS=$(dbus-daemon --session --fork --print-address)
(umask 077; exec "$BIN" --session --root "$ROOT") & SVC=$!   # UMask=0077, like its systemd unit
for _ in $(seq 50); do busctl --user status io.github.asimaos.Maintenance1 >/dev/null 2>&1 && break; sleep 0.1; done

S=$(call Status)
check "Status answers without a password"        '[[ $S == *"\\\"version\\\":\\\"2026.12.15.1\\\""* && $S == *storage* && $S == *security* ]]'
H=$(call RunHealthCheck)
check "the health check answers"                 '[[ $H == *checks* && $H == *system-files* && $H == *reserve* ]]'

# ── snapshots
N=$(call SnapshotNow | cut -d'"' -f8)
check "a snapshot of /home on request"           '[[ -n $N && $(cat $ROOT/pool/@snapshots/home/$N/asha/notes.txt) == "notes of asha" ]]'
S=$(call Status)
check "Status lists the snapshots, the update ones too" '[[ $S == *"home/$N"* && $S == *"update/2026.12.15.1"* ]]'

# ── backups (restic): set up, back up, prove a restore
if command -v restic >/dev/null; then
    call ConfigureBackup s "{\"repository\":\"$T/backup-drive/asimaos\",\"password\":\"backup-pw-1\"}" >/dev/null
    check "a backup can be set up on a folder"   '[[ -f $T/backup-drive/asimaos/config && $(stat -c %a $ROOT/etc/asimaos/restic.pw) == 600 ]]'
    call ConfigureBackup s '{"repository":"relative/path","password":"backup-pw-1"}' >/dev/null 2>&1
    check "…a bad destination is refused"        '[[ $? -ne 0 ]]'
    # An encrypted "disk": a small LUKS2 image stands in for the pool's partition (test mode).
    truncate -s 32M "$T/disk.img"
    printf 'test-disk-pw' | cryptsetup luksFormat --batch-mode --type luks2 --pbkdf pbkdf2 --pbkdf-force-iterations 1000 \
        --key-file=- "$T/disk.img" >/dev/null 2>&1
    mkdir -p "$ROOT/run/asimaos-test" && echo "$T/disk.img" > "$ROOT/run/asimaos-test/luks-device"
    call BackupNow >/dev/null
    waitfor 'grep -q "\"lastOk\": true" $ROOT/var/lib/asimaos/backup/status.json 2>/dev/null'
    check "Back up now: done, restore test passed" 'grep -q "\"lastOk\": true" $ROOT/var/lib/asimaos/backup/status.json && grep -q "\"lastTestOk\": true" $ROOT/var/lib/asimaos/backup/status.json'
    check "…with a copy of the disk-encryption header, removed from the disk after" 'grep -q "\"headerSaved\": [0-9]" $ROOT/var/lib/asimaos/backup/status.json && [[ ! -e $ROOT/var/lib/asimaos/backup/disk-header.img ]] && restic -r "$T/backup-drive/asimaos" --password-file "$ROOT/etc/asimaos/restic.pw" --no-cache -q snapshots --tag asimaos-disk-header --json | grep -q disk-header.img'
else
    echo "  skip  backups: restic is not installed here"
fi

# ── system changes: one /etc file back to its factory version at the next start
call RestoreEtc s /etc/hostname >/dev/null
check "restoring /etc/hostname is scheduled"      'grep -qx hostname $ROOT/run/initramfs/data/flags/restore-etc'
S=$(call Status)
check "…and Status shows it"                      '[[ $S == *restoreScheduled*hostname* ]]'
call RestoreEtc s /etc/shadow >/dev/null 2>&1
check "account files are protected"               '[[ $? -ne 0 ]] && ! grep -q shadow $ROOT/run/initramfs/data/flags/restore-etc'
call RestoreEtc s /etc/../root/.ssh >/dev/null 2>&1
check "paths outside /etc are refused"            '[[ $? -ne 0 ]]'

# ── Reset settings: every /etc change but the kept ones, and this person's own settings
U=$ROOT/run/initramfs/data/overlay/etc/upper
# The caller is asha (the test runs as whoever runs it: root in the container).
printf 'asha:x:%s:%s:Asha Roy:/home/asha:/bin/bash\n' "$(id -u)" "$(id -g)" > "$ROOT/etc/passwd"
mkdir -p "$U/asimaos" "$U/NetworkManager/system-connections"
echo ZONE=home > "$U/asimaos/shield.conf"; echo wifi > "$U/NetworkManager/system-connections/home.nmconnection"; echo x > "$U/passwd"
R=$(call ResetSettings)
L=$ROOT/run/initramfs/data/flags/reset-settings-etc
check "Reset settings: /etc's changes are listed for the next start" 'grep -qx asimaos/shield.conf $L && [[ $R == *'"'"'"data":[1]'"'"'* ]]'
check "…but not the accounts, the computer's name or Wi-Fi" '[[ -f $L ]] && ! grep -qE "passwd|hostname|system-connections" $L'
check "…for this person, after a copy of the old settings" 'grep -qx asha $ROOT/run/initramfs/data/flags/reset-settings && [[ -f $(echo $ROOT/run/initramfs/data/old-settings/*/etc/asimaos/shield.conf) ]]'
S=$(call Status)
check "…and Status shows it"                      '[[ $S == *"\\\"settingsReset\\\":true"* ]]'
H=$T/home-asha
mkdir -p "$H/.config/Code" "$H/.local/state" "$H/.local/share/konsole" "$H/Documents"
for f in kdeglobals dolphinrc kxkbrc plasma-localerc; do echo "$f" > "$H/.config/$f"; done
echo editor > "$H/.config/Code/settings.json"; echo mine > "$H/Documents/notes.txt"
echo view > "$H/.local/state/dolphinstaterc"; echo me > "$H/.local/share/konsole/Me.profile"
mkdir -p "$H/.local/share/plasma/look-and-feel/some.theme" "$H/.local/share/color-schemes" "$H/.icons/cursor" "$H/.local/share/fonts"
echo x > "$H/.local/share/color-schemes/Mine.colors"; echo font > "$H/.local/share/fonts/Mine.ttf"
HOME=$H USER=asha bash builder/rootfs/usr/lib/asimaos/asimaos-reset-settings home 2>/dev/null
check "the person's desktop and app settings move aside" '[[ ! -e $H/.config/kdeglobals && ! -e $H/.config/dolphinrc && ! -e $H/.local/state/dolphinstaterc && ! -e $H/.local/share/konsole ]]'
check "…and the themes, colour schemes and cursors installed since: plain KDE again" '[[ ! -e $H/.local/share/plasma && ! -e $H/.local/share/color-schemes && ! -e $H/.icons ]]'
check "…keeping files, fonts, the language, the keyboard and other apps" '[[ -f $H/Documents/notes.txt && -f $H/.local/share/fonts/Mine.ttf && -f $H/.config/kxkbrc && -f $H/.config/plasma-localerc && -f $H/.config/Code/settings.json ]]'
check "…into old-settings, nothing lost"           '[[ $(cat $H/.local/share/asimaos/old-settings/*/.config/dolphinrc) == dolphinrc ]]'

# ── TPM unlock with a PIN (D49): the PIN rules come before anything touches the disk
R=$(call EnrollTpmPin ss "test-passphrase" "12" 2>&1)
check "a TPM PIN under 4 characters is refused, with the reason" '[[ $R == *"at least 4 characters"* ]]'
R=$(call EnrollTpmPin ss "test-passphrase" "2468" 2>&1)
check "…a good PIN on a disk without encryption is refused too" '[[ $R == *"ncrypt"* ]]'

# ── factory reset staging
R=$(call PrepareFactoryReset s '{"fullName":"Asha Roy","username":"asha","password":"secret-pw"}')
check "a reset can be staged (plain disk)"       '[[ -f $ROOT/run/initramfs/data/flags/reset.json && $(stat -c %a $ROOT/run/initramfs/data/flags/reset.json) == 600 ]]'
check "…with a hashed password, never the plain one" 'grep -qF "\"passwordHash\": \"\$6\$" $ROOT/run/initramfs/data/flags/reset.json && ! grep -q secret-pw $ROOT/run/initramfs/data/flags/reset.json'
call PrepareFactoryReset s '{"fullName":"x","username":"Root!","password":"secret-pw"}' >/dev/null 2>&1
check "a bad username is refused"                '[[ $? -ne 0 ]]'
mkdir -p "$ROOT/usr/share/factory/etc" && printf 'root:x:0:0::/root:/bin/bash\nnobody:x:65534:65534::/:/usr/bin/nologin\n' > "$ROOT/usr/share/factory/etc/passwd"
call ScheduleFactoryReset >/dev/null
check "scheduling marks the staged reset"        'grep -q "\"scheduled\": true" $ROOT/run/initramfs/data/flags/reset.json'
check "…and writes the plan for the initramfs"   'grep -qx "PASSWD_LINE=asha:x:1000:1000:Asha Roy:/home/asha:/bin/bash" $ROOT/run/initramfs/data/flags/reset.plan && grep -qx ENCRYPTED=no $ROOT/run/initramfs/data/flags/reset.plan'
call CancelFactoryReset >/dev/null
check "cancelling removes it"                    '[[ ! -e $ROOT/run/initramfs/data/flags/reset.json && ! -e $ROOT/run/initramfs/data/flags/reset.plan ]]'

kill $SVC 2>/dev/null
rm -rf "$T"
echo "asimaos-maintenanced: $([[ $fails -eq 0 ]] && echo 'all checks passed' || echo "$fails check(s) failed")"
exit $(( fails > 0 ))
