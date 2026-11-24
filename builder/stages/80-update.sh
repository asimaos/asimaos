#!/usr/bin/env bash
# Stage 80: the same version as one signed .aosupd update file (D18), with the release key of
# this build (build/keys, D25). Proves the release path end to end; Asima Updater refuses it on
# a system that already runs this version (not newer) unless "Reinstall this version" is used,
# and Asima Rescue can reinstall from it.
source /work/builder/stages/common.sh

AOSUPD="${BUILD}/asimaos-dist/usr/bin/aosupd"
OUTFILE="${OUT}/asimaos-${V}-x86_64.aosupd"
CHANNEL=$(tr '[:upper:]' '[:lower:]' <<<"${ASIMAOS_RELEASE%% *}")     # "Beta 3" → beta

step "Release notes"
cat > "${BUILD}/NOTES" <<NOTES
# Asima OS ${V} (${ASIMAOS_RELEASE})
- KDE Plasma 6.8 with the KDE standard apps, plasma-login-manager
- The Asima OS apps (Updater, Maintenance, Manager, Gaming, Dev) and the installer
- Everything is on for everyone: gaming, development, virtual machines and containers
${ASIMAOS_NOTES_EXTRA:-}
NOTES

# Signed copies (Secure Boot, stage 50) when this build has them.
BOOTLOADER="${OUT}/systemd-bootx64.efi"
[[ -f ${BOOTLOADER} ]] || BOOTLOADER="${ROOT}/usr/lib/systemd/boot/efi/systemd-bootx64.efi"
RESCUE=()
[[ -f ${OUT}/asimaos-rescue_${V}.efi ]] && RESCUE=(--rescue "${OUT}/asimaos-rescue_${V}.efi")
# The shim travels with every update: installed PCs that start through shim get a newer one
# (for example signed with Microsoft's 2023 certificate) by an ordinary update.
SHIMPARTS=()
[[ -f ${OUT}/shim/shimx64.efi && -f ${OUT}/shim/mmx64.efi ]] && SHIMPARTS=(--shim "${OUT}/shim/shimx64.efi" --shim "${OUT}/shim/mmx64.efi")

step "Packing ${OUTFILE##*/} (zstd, Ed25519 key of this build)"
"${AOSUPD}" pack --key "${BUILD}/keys/release.key" --version "${V}" --channel "${CHANNEL}" \
    --expires "$(date -u -d '+120 days' +%F)" \
    --slot "${OUT}/usr_${V}.erofs" --slot "${OUT}/verity_${V}.img" --uki "${OUT}/asimaos_${V}.efi" \
    --bootloader "${BOOTLOADER}" "${SHIMPARTS[@]}" "${RESCUE[@]}" --notes "${BUILD}/NOTES" \
    -o "${OUTFILE}"
step "Checking it like Asima Updater does (signature, every part)"
"${AOSUPD}" verify --key "${BUILD}/keys/release.pub" "${OUTFILE}" > "${OUT}/asimaos-${V}-x86_64.aosupd.json"
grep -E '"(signature|partsChecked|version|channel|expires|unpackedSize)"' "${OUT}/asimaos-${V}-x86_64.aosupd.json"
ls -lh "${OUTFILE}"
check_size "${OUTFILE}" "${ASIMAOS_LIMIT_UPDATE_MB}" "The update file"
sign_outputs
