#!/usr/bin/env bash
# Stage 55: Asima Rescue, a boot image that runs entirely in RAM (apps/DESIGN.md: Asima Rescue). One
# signed UKI: the same kernel, and as its initramfs a small complete system that never switches
# to the installed one: systemd, the disk tools, aosupd and the Asima Rescue app, which draws on
# the firmware's framebuffer (Qt linuxfb, software rendering), so it works on any graphics card.
# The start menu shows it as "Asima Rescue", below the versions of Asima OS.
source /work/builder/stages/common.sh

RROOT="${BUILD}/rescue-root"
KVER="$(kver)"
DIST="${BUILD}/asimaos-dist"
RESCUE_UKI="${OUT}/asimaos-rescue_${V}.efi"
RESCUE_PACKAGES=(
    systemd systemd-sysvcompat bash coreutils util-linux grep sed gawk findutils kmod
    cryptsetup btrfs-progs dosfstools mokutil openssl tpm2-tss
    qt6-base qt6-declarative qt6-svg noto-fonts breeze-icons
)
[[ -x ${DIST}/usr/bin/asimaos-rescue ]] || die "build/asimaos-dist has no asimaos-rescue: run apps/build.sh dist first"
[[ -x ${ENGINE} ]] || die "build/services has no asimaos-engine: run apps/build.sh dist first"

step "Rescue system: packages from the same Arch archive snapshot"
rm -rf "${RROOT}"
mkdir -p "${RROOT}/var/lib/pacman" "${RROOT}/var/log" "${RROOT}/etc"
with_root_mounts pacman --root "${RROOT}" --config "${PACMAN_CONF}" --cachedir /var/cache/pacman/pkg \
    -Sy --noconfirm --needed "${RESCUE_PACKAGES[@]}" >/dev/null

step "Rescue system: Asima Rescue, its engine, aosupd and the release key"
install -Dm0755 "${DIST}/usr/bin/asimaos-rescue" "${RROOT}/usr/bin/asimaos-rescue"
install -Dm0755 "${WORK}/engines/asimaos-rescue-engine" "${RROOT}/usr/lib/asimaos/asimaos-rescue-engine"
install -Dm0755 "${ENGINE}" "${RROOT}/usr/lib/asimaos/asimaos-engine"
install -Dm0755 "${DIST}/usr/bin/aosupd" "${RROOT}/usr/bin/aosupd"
install -Dm0644 "${ROOT}/usr/share/asimaos/keys/release.pub" "${RROOT}/usr/share/asimaos/keys/release.pub"

step "Rescue system: kernel modules for disks, USB, keyboards and mice (no graphics drivers)"
# The firmware's framebuffer (simpledrm, built into the kernel) shows the screen; leaving GPU
# drivers out keeps it from being replaced, and keeps the image small.
mkdir -p "${RROOT}/usr/lib/modules/${KVER}"
(cd "${ROOT}/usr/lib/modules/${KVER}" && find . -maxdepth 1 -type f -name 'modules.*' -exec cp {} "${RROOT}/usr/lib/modules/${KVER}/" \;)
(cd "${ROOT}/usr/lib/modules/${KVER}" && tar -cf - --exclude='kernel/drivers/gpu' --exclude='kernel/drivers/media' \
    --exclude='kernel/drivers/net/wireless' --exclude='kernel/drivers/infiniband' --exclude='kernel/drivers/staging' \
    --exclude='kernel/drivers/iio' --exclude='kernel/sound' --exclude='kernel/net/bluetooth' --exclude='extramodules' \
    --exclude='updates' kernel) | tar -xf - -C "${RROOT}/usr/lib/modules/${KVER}"
with_root_mounts chroot "${RROOT}" depmod "${KVER}"

step "Rescue system: settings"
cat > "${RROOT}/etc/os-release" <<EOF
NAME="Asima Rescue"
PRETTY_NAME="Asima Rescue"
ID=asimaos-rescue
ID_LIKE=arch
VERSION_ID=${V}
IMAGE_ID=asimaos-rescue
IMAGE_VERSION=${V}
EOF
rm -f "${RROOT}/usr/lib/os-release"
echo asimaos-rescue > "${RROOT}/etc/hostname"
printf 'KEYMAP=us\n' > "${RROOT}/etc/vconsole.conf"
printf 'LANG=C.UTF-8\n' > "${RROOT}/etc/locale.conf"
# Runs as the system itself, not as an initrd (no /etc/initrd-release): the kernel starts systemd.
ln -sfn usr/lib/systemd/systemd "${RROOT}/init"
install -Dm0644 /dev/stdin "${RROOT}/usr/lib/systemd/system/asimaos-rescue.service" <<'EOF'
# Asima Rescue on the screen (tty1), full screen, as root: it only ever runs in this RAM system.
[Unit]
Description=Asima Rescue
After=systemd-udevd.service systemd-udev-trigger.service systemd-vconsole-setup.service
Conflicts=getty@tty1.service

[Service]
Environment=QT_QPA_PLATFORM=linuxfb QT_QUICK_BACKEND=software QT_QPA_FB_HIDECURSOR=0 HOME=/root
Environment=XDG_RUNTIME_DIR=/run/asimaos-rescue-gui
RuntimeDirectory=asimaos-rescue-gui
ExecStartPre=-/usr/bin/udevadm settle --timeout=15
ExecStart=/usr/bin/asimaos-rescue --fullscreen
StandardInput=tty
StandardOutput=journal
TTYPath=/dev/tty1
TTYReset=yes
TTYVTDisallocate=yes
Restart=always
RestartSec=2

[Install]
WantedBy=multi-user.target
EOF
# A terminal for experts: root on tty2 and on the serial console (nothing on the disk opens
# without its passphrase or recovery key, the same as from the screen).
for unit in getty@tty2.service serial-getty@ttyS0.service; do
    install -Dm0644 /dev/stdin "${RROOT}/usr/lib/systemd/system/${unit}.d/10-asimaos-rescue.conf" <<'EOF'
[Service]
ExecStart=
ExecStart=-/sbin/agetty --autologin root --noclear --keep-baud %I 115200,57600,38400,9600 $TERM
EOF
done
printf 'Asima Rescue %s (\\l)\n\n' "${V}" > "${RROOT}/etc/issue"
with_root_mounts chroot "${RROOT}" systemctl enable asimaos-rescue.service getty@tty2.service >/dev/null
with_root_mounts chroot "${RROOT}" systemctl set-default multi-user.target >/dev/null
with_root_mounts chroot "${RROOT}" systemctl mask systemd-firstboot.service systemd-homed.service \
    systemd-userdbd.socket systemd-networkd.service systemd-resolved.service systemd-timesyncd.service \
    systemd-remount-fs.service tmp.mount >/dev/null 2>&1 || true

step "Rescue system: leaving out what it never uses"
rm -rf "${RROOT}"/usr/share/{doc,man,info,gtk-doc,help} "${RROOT}"/usr/include "${RROOT}"/var/cache/pacman \
       "${RROOT}"/var/lib/pacman/sync "${RROOT}"/usr/lib/firmware "${RROOT}"/boot
find "${RROOT}/usr/share/locale" -mindepth 1 -maxdepth 1 ! -name 'en*' -exec rm -rf {} + 2>/dev/null || true
find "${RROOT}/usr/share/fonts" -type f ! -name 'NotoSans-*' ! -name 'NotoSansMono-*' -delete
# Icons: one Breeze set at single scale (the app tints its icons itself; Qt scales them).
rm -rf "${RROOT}/usr/share/icons/breeze-dark" "${RROOT}/usr/share/icons/breeze/breeze-icons.rcc" \
       "${RROOT}/usr/share/icons/breeze/mimetypes" "${RROOT}"/usr/lib/libKF6BreezeIcons*
find "${RROOT}/usr/share/icons" -mindepth 1 -maxdepth 1 ! -name breeze ! -name hicolor ! -name default -exec rm -rf {} +
find "${RROOT}/usr/share/icons/breeze" -mindepth 2 -maxdepth 2 \( -name '*@2x' -o -name '*@3x' \) -exec rm -rf {} +
# Mesa and LLVM come in as Qt dependencies; Rescue draws in software on the framebuffer
# (QT_QUICK_BACKEND=software) and never opens OpenGL or Vulkan.
rm -rf "${RROOT}"/usr/lib/libLLVM* "${RROOT}"/usr/lib/libgallium* "${RROOT}"/usr/lib/dri "${RROOT}"/usr/lib/gbm \
       "${RROOT}"/usr/lib/libvulkan_* "${RROOT}"/usr/share/vulkan "${RROOT}"/usr/share/glvnd/egl_vendor.d/50_mesa.json
# Build files, introspection data, locale sources: never used at run time.
rm -rf "${RROOT}"/usr/lib/cmake "${RROOT}"/usr/lib/pkgconfig "${RROOT}"/usr/share/pkgconfig "${RROOT}"/usr/share/aclocal \
       "${RROOT}"/usr/share/gir-1.0 "${RROOT}"/usr/share/i18n "${RROOT}"/usr/share/qt6/translations "${RROOT}"/usr/lib/qt6/mkspecs
find "${RROOT}/usr/lib" -name '*.a' -delete
rm -rf "${RROOT}"/usr/lib/qt6/qml/{QtWebEngine,QtQuick3D,Qt3D,QtMultimedia,QtLocation,QtCharts,QtDataVisualization} 2>/dev/null || true
# Smaller Rescue: fits PCs with little memory, and the boot partition.
# Icons: only the ones Asima Rescue and Asima UI name in their code (any quoted icon name).
grep -rhoE '"[a-z][a-z0-9-]*"' "${WORK}/apps/asimaos-rescue" "${WORK}/apps/ui" 2>/dev/null \
    | tr -d '"' | sort -u > "${BUILD}/rescue-icons.txt"
find "${RROOT}/usr/share/icons/breeze" -type f -name '*.svg' | while read -r f; do
    n=${f##*/}; n=${n%.svg}
    grep -qxF "${n}" "${BUILD}/rescue-icons.txt" || rm -f "${f}"
done
find "${RROOT}/usr/share/icons" -xtype l -delete
echo "  icons kept: $(find "${RROOT}/usr/share/icons" -name '*.svg' | wc -l)"
# Qt's build tools and build metadata; databases Rescue never reads (file types, PCI and USB names,
# character sets beyond UTF-8, time zones: Rescue runs in UTC); terminals other than the console's.
rm -rf "${RROOT}"/usr/lib/qt6/{bin,metatypes,sbom,moc,libexec} "${RROOT}"/usr/share/file "${RROOT}"/usr/share/hwdata \
       "${RROOT}"/usr/lib/gconv
find "${RROOT}/usr/share/zoneinfo" -mindepth 1 ! -name UTC ! -name Etc -exec rm -rf {} + 2>/dev/null || true
find "${RROOT}/usr/share/terminfo" -type f ! -name linux ! -name 'vt1*' ! -name vt220 ! -name dumb ! -name 'xterm*' -delete
# No network in Rescue: update files come from USB drives.
rm -rf "${RROOT}/usr/lib/modules/${KVER}/kernel/drivers/net" "${RROOT}/usr/lib/modules/${KVER}/kernel/net"
# Drivers a PC's rescue never needs: sensors, AI accelerators, regulators and power supplies, Bluetooth,
# FireWire, ISDN, virtualization hosts, watchdogs, LEDs, server storage (Fibre Channel, iSCSI) and
# network or cluster filesystems. Disks, USB, keyboards, touchpads, SD cards, TPMs and VMs stay.
M="${RROOT}/usr/lib/modules/${KVER}/kernel"
rm -rf "${M}"/drivers/{hwmon,accel,regulator,power,bluetooth,firewire,isdn,atm,w1,watchdog,vfio,vhost,xen,gnss,nfc,leds} \
       "${M}"/drivers/{memstick,pcmcia,parport,counter,fpga,cxl,ntb,target,edac,ptp,pps} \
       "${M}"/drivers/scsi/{qla2xxx,qla4xxx,lpfc,bfa,be2iscsi,bnx2fc,bnx2i,cxgbi,fcoe,fnic,csiostor,qedf,qedi,elx,libfc} \
       "${M}"/fs/{nfs,nfsd,nfs_common,lockd,smb,ceph,afs,9p,orangefs,coda,ocfs2,gfs2,dlm,netfs,cachefiles,fscache} 2>/dev/null || true
# Command-line tools that come with libraries Rescue needs (SQLite, Kerberos, SASL, LMDB, GDBM, libcap,
# libwacom); systemd's network, container, home-directory and remote-journal services.
# (pacman lists the files with the root in front; a package without programs lists none.)
for pkg in sqlite krb5 libsasl lmdb gdbm libcap libwacom; do
    { pacman --root "${RROOT}" -Qlq "${pkg}" 2>/dev/null || true; } \
        | { grep -E "^${RROOT}/usr/bin/[^/]+\$" || true; } | xargs -r rm -f
done
rm -f "${RROOT}"/usr/bin/systemd-{nspawn,vmspawn} \
      "${RROOT}"/usr/lib/systemd/systemd-{networkd,networkd-wait-online,network-generator,resolved,timesyncd,homed,homework} \
      "${RROOT}"/usr/lib/systemd/systemd-{importd,import,import-fs,export,journal-remote,journal-upload,journal-gatewayd} \
      "${RROOT}"/usr/lib/systemd/systemd-{machined,portabled,imds,imdsd,storagetm,sysupdate,sysupdated,oomd}
# Qt: Rescue imports only QtQuick.Controls.Basic and draws on the framebuffer (linuxfb), so the other
# styles, screen backends, developer plugins (QML language server, tooling, lint) and SQL drivers go,
# as does the file-type database (no file dialogs).
Q="${RROOT}/usr/lib/qt6"
rm -rf "${Q}"/qml/QtQuick/Controls/{FluentWinUI3,Fusion,Imagine,Material,Universal,designer} "${Q}"/qml/QtTest \
       "${Q}"/plugins/{qmlls,qmltooling,qmllint,sqldrivers,tls,networkinformation,platformthemes,egldeviceintegrations} \
       "${Q}"/plugins/{wayland-graphics-integration-client,wayland-shell-integration,wayland-decoration-client,xcbglintegrations} \
       "${RROOT}"/usr/share/mime "${RROOT}"/usr/share/iana-etc
find "${Q}/plugins/platforms" -type f ! -name libqlinuxfb.so ! -name libqminimal.so ! -name libqoffscreen.so -delete
with_root_mounts chroot "${RROOT}" depmod "${KVER}"
with_root_mounts chroot "${RROOT}" fc-cache -f >/dev/null 2>&1 || true
echo "rescue system: $(du -sh "${RROOT}" | cut -f1)"

step "Rescue initramfs (the whole system, zstd)"
(cd "${RROOT}" && find . -mindepth 1 | LC_ALL=C sort | bsdcpio -o -H newc -R 0:0 --quiet) \
    | zstd -T0 -15 -q > "${BUILD}/rescue-initrd.img"
ls -lh "${BUILD}/rescue-initrd.img"

step "Building ${RESCUE_UKI##*/} (signed)"
ukify build \
    --linux="${ROOT}/usr/lib/modules/${KVER}/vmlinuz" \
    --initrd="${BUILD}/rescue-initrd.img" \
    --stub="${ROOT}/usr/lib/systemd/boot/efi/linuxx64.efi.stub" \
    --uname="${KVER}" \
    --os-release="@${RROOT}/etc/os-release" \
    --cmdline="asimaos.rescue rw panic=10 lsm=landlock,lockdown,yama,integrity,bpf console=ttyS0 console=tty0" \
    --signtool=systemd-sbsign --sign-kernel --secureboot-private-key="${SB_KEY}" --secureboot-certificate="${SB_CERT}" \
    --output="${RESCUE_UKI}"
ls -lh "${RESCUE_UKI}"
if [[ -f ${SHIM}/shimx64.efi ]]; then
    python3 "${WORK}/builder/check-sbat" "${SHIM}/shimx64.efi" "${RESCUE_UKI}" \
        || die "the shim would refuse Asima Rescue's boot image (SBAT)"
fi
