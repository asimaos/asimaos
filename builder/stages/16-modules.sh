#!/usr/bin/env bash
# Stage 16: extra kernel modules (D55), built against this image's exact kernel and signed with
# this build's Secure Boot key, so they also load under lockdown (sign_module, common.sh):
#   v4l2loopback  virtual cameras (OBS Studio's virtual camera), from Arch's v4l2loopback-dkms
#   vhba          virtual CD/DVD drives for CDEmu (disc images, for older games), from Arch's
#                 vhba-module-dkms
#   xpadneo       Xbox controllers over Bluetooth (every button, rumble, battery), from its source
#                 at a pinned release (ASIMAOS_XPADNEO)
#   xone          Microsoft's Xbox Wireless Adapter and what pairs with it (controllers, headsets,
#                 chatpads), its adapter part only: wired controllers stay with the kernel's own
#                 xpad. The adapter's firmware is Microsoft's: Asima Gaming fetches it when asked.
# The kernel headers and build tools live only in this throw-away container, from the same
# archive snapshot as the image, so they are the image kernel's own.
source /work/builder/stages/common.sh

KVER="$(kver)"
DEST="${ROOT}/usr/lib/modules/${KVER}/updates/asimaos"
SRC="${BUILD}/modules-src"

step "Kernel headers ${KVER} and the modules' sources"
pacman -S --needed --noconfirm --cachedir /var/cache/pacman/pkg \
    base-devel bc git linux-headers v4l2loopback-dkms vhba-module-dkms >/dev/null
KDIR="/usr/lib/modules/${KVER}/build"
[[ -f ${KDIR}/Makefile ]] || die "linux-headers is not for the image's kernel ${KVER} ($(ls /usr/lib/modules | tr '\n' ' '))"
rm -rf "${SRC}" "${DEST}"
mkdir -p "${SRC}" "${DEST}"
cp -a /usr/src/v4l2loopback-*/. "${SRC}/v4l2loopback/"
cp -a /usr/src/vhba-module-*/. "${SRC}/vhba/"
fetch() {  # NAME "URL COMMIT TAG": the source at exactly that commit
    local url commit tag
    read -r url commit tag <<<"$2"
    git clone -q "${url}" "${SRC}/$1"
    git -C "${SRC}/$1" -c advice.detachedHead=false checkout -q "${commit}"
    [[ $(git -C "${SRC}/$1" rev-parse HEAD) == "${commit}" ]] || die "$1 is not at ${commit} (${tag})"
}
fetch xpadneo "${ASIMAOS_XPADNEO}"
fetch xone "${ASIMAOS_XONE}"

step "Building them for ${KVER}"
make -s -C "${KDIR}" M="${SRC}/v4l2loopback" -j"$(nproc)" modules
make -s -C "${KDIR}" M="${SRC}/vhba" -j"$(nproc)" modules
# xpadneo and xone come from outside the kernel and aren't promised to build on every new kernel:
# when one doesn't, the build goes on without it and says so (the end of this stage and
# modules-missing.txt in build/out). Only that driver's devices lose out: Xbox controllers over
# Bluetooth fall back to the kernel's own driver (fewer buttons, no rumble), and the Xbox Wireless
# Adapter isn't supported in that version (Asima Gaming says so; Bluetooth or a cable still work).
# v4l2loopback and vhba are Arch's own packages for this very kernel, so they must build.
KOS=("${SRC}/v4l2loopback/v4l2loopback.ko" "${SRC}/vhba/vhba.ko")
MISSING=()
if make -s -C "${SRC}/xpadneo/hid-xpadneo" KERNEL_SOURCE_DIR="${KDIR}" -j"$(nproc)" modules; then
    KOS+=("${SRC}/xpadneo/hid-xpadneo/src/hid-xpadneo.ko")
else
    MISSING+=("xpadneo (${ASIMAOS_XPADNEO##* }): Xbox controllers over Bluetooth use the kernel's own driver")
fi
if make -s -C "${KDIR}" M="${SRC}/xone" -j"$(nproc)" modules; then
    # xone's wired driver is left out: the kernel's xpad keeps wired Xbox One and Series controllers.
    rm -f "${SRC}/xone/xone_wired.ko"
    KOS+=("${SRC}"/xone/xone_*.ko)
else
    MISSING+=("xone (${ASIMAOS_XONE##* }): no driver for the Xbox Wireless Adapter")
fi

step "Signing and installing them (updates/asimaos)"
for ko in "${KOS[@]}"; do
    [[ -f ${ko} ]] || die "${ko##*/} was not built"
    strip --strip-debug "${ko}"
    zstd -q -19 -f "${ko}" -o "${DEST}/${ko##*/}.zst"
    sign_module "${DEST}/${ko##*/}.zst"
    echo "  ${ko##*/}"
done
# xpadneo takes Xbox controllers over Bluetooth from the generic driver (its module aliases), and
# keeps Steam from reading them a second time through hidraw (its udev rules); only when it built.
if [[ -f ${DEST}/hid-xpadneo.ko.zst ]]; then
    install -Dm0644 "${SRC}/xpadneo/hid-xpadneo/etc-modprobe.d/xpadneo.conf" "${ROOT}/usr/lib/modprobe.d/xpadneo.conf"
    for r in "${SRC}"/xpadneo/hid-xpadneo/etc-udev-rules.d/*.rules; do
        install -Dm0644 "${r}" "${ROOT}/usr/lib/udev/rules.d/${r##*/}"
    done
fi
# xone's own install blacklists xpad and the Wi-Fi driver mt76x2u. Neither is needed here: without
# xone's wired driver, xpad and xone never meet, and only the Xbox Wireless Adapter is taken from
# mt76x2u (which knows its chip), so USB Wi-Fi sticks with that chip keep working
# (udev/70-asimaos-controllers.rules).
with_root_mounts chroot "${ROOT}" depmod "${KVER}"
SIGNED=(v4l2loopback vhba)
[[ -f ${DEST}/hid-xpadneo.ko.zst ]] && SIGNED+=(hid_xpadneo)
[[ -f ${DEST}/xone_dongle.ko.zst ]] && SIGNED+=(xone_gip xone_dongle xone_gip_gamepad)
for m in "${SIGNED[@]}"; do
    with_root_mounts chroot "${ROOT}" modinfo -k "${KVER}" -F sig_id "${m}" | grep PKCS >/dev/null || die "${m} is not signed"
done
with_root_mounts chroot "${ROOT}" modinfo -k "${KVER}" -n xpad | grep /kernel/drivers/input/joystick/ >/dev/null \
    || die "xpad isn't the kernel's own"
[[ ! -e ${DEST}/xone_wired.ko.zst ]] || die "xone's wired driver must not be installed"
rm -rf "${SRC}"
# What this version lacks, said plainly at the end of the stage and kept with the build's files.
rm -f "${OUT}/modules-missing.txt"
if ((${#MISSING[@]})); then
    printf '%s\n' "${MISSING[@]}" > "${OUT}/modules-missing.txt"
    printf '  WARNING: not built for %s: %s\n' "${KVER}" "${MISSING[@]}" >&2
else
    echo "  every extra module built for ${KVER}"
fi
