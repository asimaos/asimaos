#!/usr/bin/env bash
# Stage 70: Asima OS live/installer ISO (UEFI only, hybrid: works as CD image and USB stick).
#   ESP (El Torito + GPT partition): systemd-boot + live UKI (asimaos.live)
#   ISO9660 /asimaos: the same sealed /usr image + installer payload
source /work/builder/stages/common.sh

KVER="$(kver)"
USRHASH="$(cat "${OUT}/usrhash_${V}.txt")"
ISO_ROOT="${BUILD}/iso"
ESP_IMG="${BUILD}/iso-esp.img"
ISO="${OUT}/asimaos-${V}-x86_64.iso"
LIVE_UKI="${BUILD}/asimaos-live_${V}.efi"
SDBOOT="${OUT}/systemd-bootx64.efi"          # signed (stage 50)

step "Building live UKI"
ukify build \
    --linux="${ROOT}/usr/lib/modules/${KVER}/vmlinuz" \
    --initrd="${BUILD}/initrd.img" \
    --stub="${ROOT}/usr/lib/systemd/boot/efi/linuxx64.efi.stub" \
    --uname="${KVER}" \
    --os-release="@${ROOT}/usr/lib/os-release" \
    --cmdline="root=tmpfs rootflags=mode=0755 asimaos.live asimaos.version=${V} asimaos.usrhash=${USRHASH} rd.luks=no rw lsm=landlock,lockdown,yama,integrity,apparmor,bpf quiet splash plymouth.ignore-serial-consoles console=ttyS0 console=tty0" \
    --signtool=systemd-sbsign --sign-kernel --secureboot-private-key="${SB_KEY}" --secureboot-certificate="${SB_CERT}" \
    --output="${LIVE_UKI}"

step "Building EFI boot image"
rm -f "${ESP_IMG}"
ESP_KIB=$(( ( $(stat -c %s "${LIVE_UKI}") + $(stat -c %s "${SDBOOT}") * 2 ) / 1024 + 20480 ))
mkfs.vfat -C -n ASIMAOS_ESP "${ESP_IMG}" "${ESP_KIB}" >/dev/null
printf 'default asimaos-live.conf\ntimeout 3\neditor no\nconsole-mode keep\n' > "${BUILD}/iso-loader.conf"
printf '# Asima OS start menu entry (live USB).\ntitle    Try or install Asima OS\nversion  %s\nsort-key asimaos-0\nuki      /EFI/Asima/asimaos-live.efi\n' \
    "${V}" > "${BUILD}/iso-entry.conf"
mmd   -i "${ESP_IMG}" ::/EFI ::/EFI/BOOT ::/EFI/Asima ::/loader ::/loader/entries
if [[ -f ${OUT}/shim/shimx64.efi ]]; then     # Secure Boot with Microsoft's keys (D24)
    mcopy -i "${ESP_IMG}" "${OUT}/shim/shimx64.efi" ::/EFI/BOOT/BOOTX64.EFI
    mcopy -i "${ESP_IMG}" "${OUT}/shim/mmx64.efi" ::/EFI/BOOT/mmx64.efi
    mcopy -i "${ESP_IMG}" "${SDBOOT}" ::/EFI/BOOT/grubx64.efi
else
    mcopy -i "${ESP_IMG}" "${SDBOOT}" ::/EFI/BOOT/BOOTX64.EFI
fi
mcopy -i "${ESP_IMG}" "${OUT}/shim/asimaos.der" ::/EFI/BOOT/asimaos.der
mcopy -i "${ESP_IMG}" "${LIVE_UKI}" ::/EFI/Asima/asimaos-live.efi
mcopy -i "${ESP_IMG}" "${BUILD}/iso-loader.conf" ::/loader/loader.conf
mcopy -i "${ESP_IMG}" "${BUILD}/iso-entry.conf" ::/loader/entries/asimaos-live.conf

step "Staging ISO payload (sealed /usr + installer)"
rm -rf "${ISO_ROOT}"
mkdir -p "${ISO_ROOT}/asimaos/slots" "${ISO_ROOT}/asimaos/uki"
cp --reflink=auto "${OUT}/usr_${V}.erofs" "${OUT}/verity_${V}.img" "${ISO_ROOT}/asimaos/slots/"
cp "${OUT}/asimaos_${V}.efi" "${ISO_ROOT}/asimaos/uki/"
[[ -f ${OUT}/asimaos-rescue_${V}.efi ]] && cp "${OUT}/asimaos-rescue_${V}.efi" "${ISO_ROOT}/asimaos/uki/asimaos-rescue.efi"
cp "${SDBOOT}" "${ISO_ROOT}/asimaos/systemd-bootx64.efi"
cp -r "${OUT}/shim" "${ISO_ROOT}/asimaos/shim"      # the installer's Secure Boot files
cp -r "${WORK}/builder/repart.d" "${ISO_ROOT}/asimaos/repart.d"
install -m 0755 "${WORK}/engines/asimaos-install" "${ISO_ROOT}/asimaos/asimaos-install"
install -m 0755 "${WORK}/engines/asimaos-repair" "${ISO_ROOT}/asimaos/asimaos-repair"
# The engine they start: on the stick only, never in the installed /usr.
[[ -x ${ENGINE} ]] || die "build/services has no asimaos-engine: run apps/build.sh dist first"
install -m 0755 "${ENGINE}" "${ISO_ROOT}/asimaos/asimaos-engine"
echo "${V}" > "${ISO_ROOT}/asimaos/VERSION"
# The migration steps an update file would carry (D52), for "Repair this computer" when it
# installs this stick's version on a computer whose settings come from an older one.
migration_steps
mkdir -p "${ISO_ROOT}/asimaos/migrations"
((${#STEPS[@]})) && cp "${STEPS[@]}" "${ISO_ROOT}/asimaos/migrations/"
echo "${MIN_FROM}" > "${ISO_ROOT}/asimaos/migrations/steps-from"
# Every program on this disc, its licences and where its sources are.
cp -r "${ROOT}/usr/share/asimaos/licences" "${ISO_ROOT}/licences"

step "Writing ${ISO##*/}"
rm -f "${ISO}"
xorriso -as mkisofs \
    -iso-level 3 -full-iso9660-filenames -joliet -joliet-long -rational-rock \
    -volid ASIMAOS_LIVE \
    -partition_offset 16 \
    -append_partition 2 C12A7328-F81F-11D2-BA4B-00A0C93EC93B "${ESP_IMG}" \
    -appended_part_as_gpt \
    -eltorito-alt-boot -e --interval:appended_partition_2:all:: -no-emul-boot \
    -output "${ISO}" \
    "${ISO_ROOT}"
ls -lh "${ISO}"
check_size "${ISO}" "${ASIMAOS_LIMIT_ISO_MB}" "The ISO"
sign_outputs
