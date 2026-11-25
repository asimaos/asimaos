#!/usr/bin/env bash
# Stage 16: extra kernel modules (D55), built against this image's exact kernel and signed with
# this build's Secure Boot key, so they also load under lockdown (sign_module, common.sh):
#   v4l2loopback  virtual cameras (OBS Studio's virtual camera), from Arch's v4l2loopback-dkms
#   vhba          virtual CD/DVD drives for CDEmu (disc images, for older games), from Arch's
#                 vhba-module-dkms
#   xpadneo       Xbox controllers over Bluetooth (every button, rumble, battery), from its source
#                 at a pinned release (ASIMAOS_XPADNEO)
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
read -r url commit tag <<<"${ASIMAOS_XPADNEO}"
git clone -q "${url}" "${SRC}/xpadneo"
git -C "${SRC}/xpadneo" -c advice.detachedHead=false checkout -q "${commit}"
[[ $(git -C "${SRC}/xpadneo" rev-parse HEAD) == "${commit}" ]] || die "xpadneo is not at ${commit} (${tag})"

step "Building them for ${KVER}"
make -s -C "${KDIR}" M="${SRC}/v4l2loopback" -j"$(nproc)" modules
make -s -C "${KDIR}" M="${SRC}/vhba" -j"$(nproc)" modules
make -s -C "${SRC}/xpadneo/hid-xpadneo" KERNEL_SOURCE_DIR="${KDIR}" -j"$(nproc)" modules

step "Signing and installing them (updates/asimaos)"
for ko in "${SRC}/v4l2loopback/v4l2loopback.ko" "${SRC}/vhba/vhba.ko" "${SRC}/xpadneo/hid-xpadneo/src/hid-xpadneo.ko"; do
    [[ -f ${ko} ]] || die "${ko##*/} was not built"
    strip --strip-debug "${ko}"
    zstd -q -19 -f "${ko}" -o "${DEST}/${ko##*/}.zst"
    sign_module "${DEST}/${ko##*/}.zst"
    echo "  ${ko##*/}"
done
# xpadneo takes Xbox controllers over Bluetooth from the generic driver (its module aliases), and
# keeps Steam from reading them a second time through hidraw (its udev rules).
install -Dm0644 "${SRC}/xpadneo/hid-xpadneo/etc-modprobe.d/xpadneo.conf" "${ROOT}/usr/lib/modprobe.d/xpadneo.conf"
for r in "${SRC}"/xpadneo/hid-xpadneo/etc-udev-rules.d/*.rules; do
    install -Dm0644 "${r}" "${ROOT}/usr/lib/udev/rules.d/${r##*/}"
done
with_root_mounts chroot "${ROOT}" depmod "${KVER}"
for m in v4l2loopback vhba hid_xpadneo; do
    with_root_mounts chroot "${ROOT}" modinfo -k "${KVER}" -F sig_id "${m}" | grep PKCS >/dev/null || die "${m} is not signed"
done
rm -rf "${SRC}"
