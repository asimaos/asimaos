#!/usr/bin/env bash
# Boot Asima OS in QEMU (UEFI).
#
#   testbed/run-qemu.sh [options] [ISO or disk image]
#     path/to/asimaos.iso   the live ISO, with the test disk build/vm-disk.raw (64 GB) attached
#     path/to/disk.raw        a prebuilt disk image
#     (nothing)               the installed test disk build/vm-disk.raw
#
#   --disc FILE     an extra DVD, e.g. the updates disc asimaos-updates-test.iso (it shows up in
#                   Dolphin and in Asima Updater's file dialog)
#   --secureboot    Secure Boot on, with Microsoft's keys like a new PC: the first start shows the
#                   blue MokManager screen, where the Asima OS key is approved once. These firmware
#                   settings are kept in build/ovmf-vars-sb.qcow2, the normal ones in
#                   build/ovmf-vars.qcow2
#   --fresh         forget the firmware settings first (boot entries, an approved key)
#
# Live ISO: the "live" session signs in by itself and the installer opens. Installed system: the
# account and passphrase you chose in the installer (root is locked). Prebuilt disk image (TEST
# ONLY): LUKS passphrase "asimaos", console login root / asimaos. Without a display QEMU uses this
# terminal (exit: Ctrl-a then x).
set -euo pipefail
cd "$(dirname "$(readlink -f "$0")")/.."   # the repository root

TARGET="" SECUREBOOT=no FRESH=no
DISCS=()
while [[ $# -gt 0 ]]; do
    case "$1" in
        --disc) DISCS+=("$(readlink -f "${2:?--disc needs a file}")"); shift ;;
        --secureboot) SECUREBOOT=yes ;;
        --fresh) FRESH=yes ;;
        -h|--help) sed -n '2,21p' "$0"; exit 0 ;;
        -*) echo "unknown option: $1" >&2; exit 2 ;;
        *) TARGET=$(readlink -f "$1") ;;
    esac
    shift
done
TARGET=${TARGET:-build/vm-disk.raw}
mkdir -p build

if [[ ${SECUREBOOT} == yes ]]; then
    MACHINE=q35,accel=kvm,smm=on
    OVMF_CODE=/usr/share/edk2/ovmf/OVMF_CODE_4M.secboot.qcow2
    OVMF_VARS=build/ovmf-vars-sb.qcow2
    VARS_TEMPLATE=/usr/share/edk2/ovmf/OVMF_VARS_4M.secboot.qcow2   # Microsoft's keys enrolled
    FIRMWARE_EXTRA=(-global driver=cfi.pflash01,property=secure,value=on)
else
    MACHINE=q35,accel=kvm
    OVMF_CODE=/usr/share/edk2/ovmf/OVMF_CODE_4M.qcow2
    OVMF_VARS=build/ovmf-vars.qcow2
    VARS_TEMPLATE=/usr/share/edk2/ovmf/OVMF_VARS_4M.qcow2
    FIRMWARE_EXTRA=()
fi
[[ ${FRESH} == no && -f ${OVMF_VARS} ]] || cp "${VARS_TEMPLATE}" "${OVMF_VARS}"

DRIVES=()
if [[ ${TARGET} == *.iso ]]; then
    # The installer needs 48 GB and updates 10 GB free next to the installed version: the
    # test disk is 64 GB (a sparse file: it only takes the space that is written).
    truncate -s '>64G' build/vm-disk.raw
    DRIVES+=(-drive "file=build/vm-disk.raw,format=raw,if=virtio,discard=unmap")
    DRIVES+=(-drive "file=${TARGET},media=cdrom,readonly=on")
else
    [[ -f ${TARGET} ]] || { echo "no disk image at ${TARGET} (install from the ISO first)" >&2; exit 1; }
    DRIVES+=(-drive "file=${TARGET},format=raw,if=virtio,discard=unmap")
fi
for disc in "${DISCS[@]}"; do
    DRIVES+=(-drive "file=${disc},media=cdrom,readonly=on")
done

if [[ -n "${WAYLAND_DISPLAY:-}${DISPLAY:-}" ]]; then
    DISPLAY_ARGS=(-display gtk -vga virtio -device virtio-tablet-pci -serial mon:stdio)
else
    DISPLAY_ARGS=(-nographic)
fi

exec qemu-system-x86_64 \
    -machine "${MACHINE}" -cpu host -m 4096 -smp 4 \
    "${FIRMWARE_EXTRA[@]}" \
    -drive if=pflash,format=qcow2,unit=0,readonly=on,file="${OVMF_CODE}" \
    -drive if=pflash,format=qcow2,unit=1,file="${OVMF_VARS}" \
    "${DRIVES[@]}" \
    "${DISPLAY_ARGS[@]}"
