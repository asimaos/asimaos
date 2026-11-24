#!/usr/bin/env bash
# Stage 20: build the generic systemd-based initramfs with mkinitcpio (inside the image tree).
source /work/builder/stages/common.sh

KVER="$(kver)"
step "Building initramfs for kernel ${KVER}"
with_root_mounts chroot "${ROOT}" mkinitcpio -k "${KVER}" -g /asimaos-initrd.img
mv "${ROOT}/asimaos-initrd.img" "${BUILD}/initrd.img"
ls -lh "${BUILD}/initrd.img"
