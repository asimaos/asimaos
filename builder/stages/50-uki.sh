#!/usr/bin/env bash
# Stage 50: assemble the Unified Kernel Image (kernel + initramfs + cmdline pinned to this /usr),
# signed for Secure Boot with this build's key, and the signed boot loader. With Fedora's
# Microsoft-signed shim (D24) in front, any PC with Microsoft's keys starts it once the Asima OS key
# is approved on the blue MokManager screen (or ahead of time in Asima Maintenance → Security).
source /work/builder/stages/common.sh

KVER="$(kver)"
USRHASH="$(cat "${OUT}/usrhash_${V}.txt")"
# lsm=: the kernel's default security modules plus AppArmor (docs/architecture.md §11).
# efi_pstore: a kernel crash leaves its last messages in an EFI variable; systemd-pstore keeps
# them in /var/lib/systemd/pstore at the next start, for Asima Maintenance → Health.
# systemd.gpt_auto=0: nothing mounts the ESP by itself (/etc/fstab).
CMDLINE="root=tmpfs rootflags=mode=0755 asimaos.version=${V} asimaos.usrhash=${USRHASH} rw panic=10 efi_pstore.pstore_disable=0 systemd.gpt_auto=0 lsm=landlock,lockdown,yama,integrity,apparmor,bpf quiet splash plymouth.ignore-serial-consoles console=ttyS0 console=tty0"

step "Building UKI asimaos_${V}.efi"
ukify build \
    --linux="${ROOT}/usr/lib/modules/${KVER}/vmlinuz" \
    --initrd="${BUILD}/initrd.img" \
    --stub="${ROOT}/usr/lib/systemd/boot/efi/linuxx64.efi.stub" \
    --uname="${KVER}" \
    --os-release="@${ROOT}/usr/lib/os-release" \
    --cmdline="${CMDLINE}" \
    --signtool=systemd-sbsign --sign-kernel --secureboot-private-key="${SB_KEY}" --secureboot-certificate="${SB_CERT}" \
    --output="${OUT}/asimaos_${V}.efi"

step "Signed boot loader and Secure Boot files for the disk image, the ISO and updates"
sign_efi "${ROOT}/usr/lib/systemd/boot/efi/systemd-bootx64.efi" "${OUT}/systemd-bootx64.efi"
rm -rf "${OUT}/shim"
mkdir -p "${OUT}/shim"
openssl x509 -in "${SB_CERT}" -outform DER -out "${OUT}/shim/asimaos.der"
if [[ -f ${SHIM}/shimx64.efi && -f ${SHIM}/mmx64.efi ]]; then
    cp "${SHIM}/shimx64.efi" "${SHIM}/mmx64.efi" "${OUT}/shim/"
    echo "  with shim: $(cat "${SHIM}/VERSION" 2>/dev/null || echo unknown version)"
    # Shim starts only programs with SBAT data newer than its revocations.
    python3 "${WORK}/builder/check-sbat" "${SHIM}/shimx64.efi" "${OUT}/systemd-bootx64.efi" "${OUT}/asimaos_${V}.efi" \
        || die "the shim would refuse this build's boot loader or boot image (SBAT)"
else
    echo "  no shim: the boot loader starts directly (this build's key must be in the firmware)"
fi

ukify inspect "${OUT}/asimaos_${V}.efi" | grep -E '^\.(osrel|cmdline|uname|linux|initrd)|size' | head -24 || true
