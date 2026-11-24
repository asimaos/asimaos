#!/usr/bin/env bash
# Stage 60: lay out a whole-disk Asima OS install image (what the Live Installer will do on real disks):
#   p1 ESP (7 GB FAT32): systemd-boot, the UKI in EFI/Asima, the start menu entry
#      "Asima OS (current)" with boot counting (+3-0), and Rescue when it was built
#   p2 crypt_pool: LUKS2 -> Btrfs with 10 subvolumes, /usr slot image in @slots
source /work/builder/stages/common.sh

DISK="${OUT}/asimaos-disk.raw"
KEYFILE="${BUILD}/test-luks.key"

step "Staging ESP and pool contents"
rm -rf "${STAGE}"
mkdir -p "${STAGE}/esp/EFI/BOOT" "${STAGE}/esp/EFI/systemd" "${STAGE}/esp/EFI/Asima" "${STAGE}/esp/loader" \
         "${STAGE}/pool/@slots" "${STAGE}/pool/@data/overlay/etc/upper" "${STAGE}/pool/@data/overlay/etc/work" \
         "${STAGE}/pool/@data/flags"

# Secure Boot (stage 50): with shim, shim is BOOTX64.EFI and starts grubx64.efi (systemd-boot,
# signed with this build's key); the key's certificate stays on the ESP for MokManager.
SDBOOT="${OUT}/systemd-bootx64.efi"
if [[ -f ${OUT}/shim/shimx64.efi ]]; then
    cp "${OUT}/shim/shimx64.efi" "${STAGE}/esp/EFI/BOOT/BOOTX64.EFI"
    cp "${OUT}/shim/mmx64.efi" "${STAGE}/esp/EFI/BOOT/mmx64.efi"
    cp "${SDBOOT}" "${STAGE}/esp/EFI/BOOT/grubx64.efi"
else
    cp "${SDBOOT}" "${STAGE}/esp/EFI/BOOT/BOOTX64.EFI"
fi
cp "${OUT}/shim/asimaos.der" "${STAGE}/esp/EFI/BOOT/asimaos.der"
cp "${SDBOOT}" "${STAGE}/esp/EFI/systemd/systemd-bootx64.efi"
# The start menu (boot.rs): the version starts with 3 tries as "Asima OS (current)";
# systemd-bless-boot removes the counter after a good start.
cp "${OUT}/asimaos_${V}.efi" "${STAGE}/esp/EFI/Asima/asimaos_${V}.efi"
[[ -f ${OUT}/asimaos-rescue_${V}.efi ]] && cp "${OUT}/asimaos-rescue_${V}.efi" "${STAGE}/esp/EFI/Asima/asimaos-rescue.efi"
AOSUPD="${BUILD}/asimaos-dist/usr/bin/aosupd"
"${AOSUPD}" boot promote --esp "${STAGE}/esp" --version "${V}" >/dev/null
"${AOSUPD}" boot rescue --esp "${STAGE}/esp"
"${AOSUPD}" boot loader-conf --esp "${STAGE}/esp" > "${STAGE}/esp/loader/loader.conf"
ls "${STAGE}/esp/loader/entries"

cp --reflink=auto "${OUT}/usr_${V}.erofs" "${OUT}/verity_${V}.img" "${STAGE}/pool/@slots/"
# A copy of the boot image next to the system image, for "Repair this computer".
mkdir -p "${STAGE}/pool/@slots/boot"
cp --reflink=auto "${OUT}/asimaos_${V}.efi" "${STAGE}/pool/@slots/boot/"

step "Creating disk image with systemd-repart (offline, LUKS2 encrypted pool)"
printf '%s' "${ASIMAOS_TEST_LUKS_PASSPHRASE}" > "${KEYFILE}"
rm -f "${DISK}"
systemd-repart \
    --offline=yes \
    --empty=create \
    --size=24G \
    --dry-run=no \
    --definitions="${WORK}/builder/repart.d" \
    --copy-source="${STAGE}" \
    --key-file="${KEYFILE}" \
    "${DISK}"

ls -lhs "${DISK}"
