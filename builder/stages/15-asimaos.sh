#!/usr/bin/env bash
# Stage 15: everything Asima OS adds on top of the packages: the Asima OS apps and services
# (build/asimaos-dist, made by "apps/build.sh dist"), the release key, the NVIDIA check for
# Flatpak apps, the services that start with the system (everything is on for everyone, D26), and
# the system defaults.
source /work/builder/stages/common.sh

DIST="${BUILD}/asimaos-dist"
[[ -x ${DIST}/usr/lib/asimaos/asimaos-updaterd ]] || die "build/asimaos-dist is missing: run apps/build.sh dist first"

step "Installing the Asima OS apps and services"
cp -a "${DIST}/." "${ROOT}/"
rm -f "${ROOT}/usr/bin/asimaos-rescue"      # only in the Asima Rescue boot image (stage 55)
for unit in "${ROOT}"/usr/lib/systemd/system/asimaos-*.service; do
    grep -q '^ExecStart=/usr/lib/asimaos/' "${unit}" || die "unexpected unit ${unit##*/}"
done

step "Release key of this build (D25: made once in build/keys, never in the repository)"
if [[ ! -f ${KEYS}/release.key ]]; then
    mkdir -p "${KEYS}"
    (umask 077; "${DIST}/usr/bin/aosupd" keygen "${KEYS}/release")
fi
install -Dm0644 "${KEYS}/release.pub" "${ROOT}/usr/share/asimaos/keys/release.pub"
# The next release key, when one is prepared: systems from this build then also accept
# updates signed with it, so the key can change on a plan (aosupd keygen build/keys/release-next).
if [[ -f ${KEYS}/release-next.pub ]]; then
    install -Dm0644 "${KEYS}/release-next.pub" "${ROOT}/usr/share/asimaos/keys/release-next.pub"
fi

step "Secure Boot key of this build (D25): signs the boot loader and every boot image"
if [[ ! -f ${SB_KEY} ]]; then
    (umask 077; openssl req -new -x509 -newkey rsa:2048 -sha256 -nodes -days 3650 -keyout "${SB_KEY}" -out "${SB_CERT}" \
        -subj "/CN=Asima OS Secure Boot $(date -u +%F) $(openssl rand -hex 4)/" 2>/dev/null)
fi
# Asima Maintenance checks with it whether the key is approved (mokutil --test-key).
openssl x509 -in "${SB_CERT}" -outform DER -out "${ROOT}/usr/share/asimaos/keys/asimaos.der"
chmod 0644 "${ROOT}/usr/share/asimaos/keys/asimaos.der"
openssl x509 -in "${SB_CERT}" -noout -subject

step "The IOMMU boot add-on for GPU passthrough, signed with this build's key"
# Used only on PCs where someone gives a graphics card to virtual machines (Asima Dev): copied to
# the ESP's loader/addons then, so systemd-stub adds these kernel options. It is measured into
# PCR 12, which TPM unlock doesn't use.
install -d "${ROOT}/usr/lib/asimaos/addons"
ukify build \
    --stub="${ROOT}/usr/lib/systemd/boot/efi/addonx64.efi.stub" \
    --cmdline="intel_iommu=on amd_iommu=on iommu=pt" \
    --signtool=systemd-sbsign --secureboot-private-key="${SB_KEY}" --secureboot-certificate="${SB_CERT}" \
    --output="${ROOT}/usr/lib/asimaos/addons/asimaos-iommu.addon.efi"

step "NVIDIA drivers signed with this build's key, so they also load under lockdown"
# Arch's NVIDIA modules carry no signature. Signed here (sign-file's format: the module, a
# detached PKCS#7 signature, struct module_signature, the magic), they load wherever the kernel
# trusts this key: on PCs where it was approved on the blue MokManager screen. The initramfs only
# locks the kernel down there (asimaos-early-mount).
sign_module() {
    local ko=$1 tmp len
    tmp=$(mktemp -d)
    zstd -dq "${ko}" -o "${tmp}/m.ko"
    if tail -c 28 "${tmp}/m.ko" | grep -q 'Module signature appended'; then
        rm -rf "${tmp}"; return 0                  # already signed (stage run again)
    fi
    openssl cms -sign -in "${tmp}/m.ko" -signer "${SB_CERT}" -inkey "${SB_KEY}" -binary -noattr -nocerts \
        -nosmimecap -md sha256 -outform DER -out "${tmp}/sig"
    len=$(stat -c %s "${tmp}/sig")
    {
        cat "${tmp}/m.ko" "${tmp}/sig"
        printf '\0\0\2\0\0\0\0\0'                 # algo, hash, id_type = PKEY_ID_PKCS7, lengths, pad
        printf "\\x$(printf %02x $((len >> 24 & 255)))\\x$(printf %02x $((len >> 16 & 255)))"
        printf "\\x$(printf %02x $((len >> 8 & 255)))\\x$(printf %02x $((len & 255)))"
        printf '~Module signature appended~\n'
    } > "${tmp}/signed.ko"
    zstd -q -19 -f "${tmp}/signed.ko" -o "${ko}"
    rm -rf "${tmp}"
}
n=0
for ko in "${ROOT}"/usr/lib/modules/*/extramodules/nvidia*.ko.zst; do
    sign_module "${ko}"
    n=$((n + 1))
done
(( n >= 4 )) || die "the NVIDIA modules were not found"
echo "  signed ${n} NVIDIA modules"

step "NVIDIA for Flatpak apps: Flathub must have the GL extension for this driver version"
# Flatpak apps (Steam, Heroic, Lutris…) only get the NVIDIA driver from Flathub's extension of
# the very same version. A public release waits for it; alphas and betas only warn.
# The driver version, from pacman's database, or (stage 15 run again after stage 30 removed it)
# from the name of NVIDIA's GL library.
NV=$(pacman --root "${ROOT}" -Q nvidia-utils 2>/dev/null | awk '{ print $2 }' || true)
NV=${NV%-*}
if [[ -z ${NV} ]]; then
    NV=$(ls "${ROOT}"/usr/lib/libnvidia-glcore.so.* | sed -n 's/.*libnvidia-glcore\.so\.//p' | head -n1)
fi
[[ ${NV} =~ ^[0-9]+\.[0-9]+(\.[0-9]+)?$ ]] || die "the NVIDIA driver version was not found"
NV=${NV//./-}
REF="org.freedesktop.Platform.GL.nvidia-${NV}"
code=$(curl -s -o /dev/null -w '%{http_code}' --max-time 30 \
    "https://dl.flathub.org/repo/refs/heads/runtime/${REF}/x86_64/1.4" || true)
case ${code} in
    200) echo "  Flathub has ${REF}" ;;
    404) if [[ ${ASIMAOS_RELEASE} == Alpha* || ${ASIMAOS_RELEASE} == Beta* ]]; then
             echo "  WARNING: Flathub has no ${REF} yet: Flatpak games would run without the NVIDIA driver"
         else
             die "Flathub has no ${REF} yet: wait for it before a release"
         fi ;;
    *) echo "  WARNING: Flathub could not be reached (${code:-no answer}); not checked" ;;
esac

step "Services that start with the system"
with_root_mounts chroot "${ROOT}" systemctl enable \
    NetworkManager.service NetworkManager-dispatcher.service systemd-resolved.service systemd-timesyncd.service bluetooth.service \
    plasmalogin.service cups.socket avahi-daemon.socket power-profiles-daemon.service \
    switcheroo-control.service scx_loader.service apparmor.service fstrim.timer \
    systemd-oomd.service \
    asimaos-shield.service asimaos-migrate.service asimaos-finish-update.service \
    asimaos-core-health.service asimaos-desktop-health.service asimaos-boot-timeout.timer \
    asimaos-snapshot.timer asimaos-backup.timer asimaos-scrub.timer asimaos-balance.timer asimaos-ssd-audit.timer \
    thermald.service var-cache.mount var-cache-apparmor.mount asimaos-keyboard-sync.path asimaos-keyboard-sync.service \
    systemd-pstore.service asimaos-passthrough-bind.service asimaos-battery.service \
    asimaos-reset-settings.service asimaos-nvidia-flatpak.service asimaos-tpm-rotation.service asimaos-face-sync.service \
    asimaos-secureboot-db.path
# Snap and Nix start when first used (D28). Waydroid's container starts once Android is set up
# in Asima Dev (it downloads the Android system then).
with_root_mounts chroot "${ROOT}" systemctl enable snapd.socket snapd.apparmor.service nix-daemon.socket
# Handhelds' built-in controls (D55): InputPlumber manages only the handhelds its files name, and
# Asima Gaming chooses which controller it shows to Steam and the games.
with_root_mounts chroot "${ROOT}" systemctl enable inputplumber.service inputplumber-suspend.service
# Virtual machines and containers for everyone (D26): libvirt starts when first used, and
# Podman's Docker-compatible socket is there for IDEs and dev containers.
with_root_mounts chroot "${ROOT}" systemctl enable \
    virtqemud.socket virtqemud-ro.socket virtqemud-admin.socket virtnetworkd.socket virtnetworkd-ro.socket \
    virtstoraged.socket virtstoraged-ro.socket virtnodedevd.socket virtnodedevd-ro.socket \
    virtsecretd.socket virtsecretd-ro.socket virtlogd.socket virtlockd.socket
with_root_mounts chroot "${ROOT}" systemctl --global enable podman.socket asimaos-desktop-check.service asimaos-dev-storage.service
# The system's virtual machines, networks, storage pools and secrets (qemu:///system) are defined
# in @vm next to their disks (D53): libvirt's factory definitions, as the package ships them, move
# to /usr, and the early mount binds /var/lib/libvirt/definitions/<folder> over the empty folders
# left in /etc/libvirt, so Reset settings never holds them.
DEFS="${ROOT}/usr/share/asimaos/libvirt-definitions"
rm -rf "${DEFS}" && mkdir -p "${DEFS}"
for d in qemu storage secrets; do
    if [[ -d ${ROOT}/etc/libvirt/${d} ]]; then
        cp -a "${ROOT}/etc/libvirt/${d}" "${DEFS}/${d}"
        find "${ROOT}/etc/libvirt/${d}" -mindepth 1 -delete
    else
        install -d -m 0700 "${DEFS}/${d}" "${ROOT}/etc/libvirt/${d}"
    fi
done
chmod 0700 "${DEFS}/secrets" "${ROOT}/etc/libvirt/secrets"
# Guest tools (D22): each starts only inside its own kind of virtual machine. SPICE (clipboard,
# screen size, shared folders) and the QEMU guest agent start when QEMU offers their channel.
with_root_mounts chroot "${ROOT}" systemctl enable \
    vboxservice.service vmtoolsd.service vmware-vmblock-fuse.service hv_kvp_daemon.service hv_vss_daemon.service
with_root_mounts chroot "${ROOT}" systemctl set-default graphical.target

step "System defaults"
# Administrators (group wheel, D10) may use sudo; polkit already treats wheel as admin.
install -Dm0440 /dev/stdin "${ROOT}/etc/sudoers.d/10-asimaos-wheel" <<'EOF'
# Asima OS: the account made by the installer is an administrator (group wheel).
%wheel ALL=(ALL:ALL) ALL
EOF
# DNS through systemd-resolved: /etc/resolv.conf never changes, so it never shows up as a
# changed /etc file in Asima Maintenance.
ln -sfn ../run/systemd/resolve/stub-resolv.conf "${ROOT}/etc/resolv.conf"
# The start-up screen: Asima OS's own (D51), in the read-only image like its choice
# (/usr/share/plymouth/plymouthd.defaults, builder/rootfs); it asks for the disk passphrase, Esc
# shows the messages. The desktop keeps KDE's own look: nothing is set for it here. The plymouth
# initramfs hook (stage 20) copies the theme, with its font.
T="${ROOT}/usr/share/plymouth/themes/asimaos" S="${ROOT}/usr/share/plymouth/themes/spinner"
[[ -f ${T}/asimaos.plymouth && -f ${S}/entry.png ]] || die "the start-up theme or Plymouth's spinner images are missing"
# The spinner, the passphrase box, the lock, and the Caps Lock and keyboard-layout hints: Plymouth's own.
cp "${S}"/throbber-*.png "${S}"/animation-*.png "${S}"/{bullet,capslock,entry,keyboard,keymap-render,lock}.png "${T}/"
# The mark and the name in the middle, drawn at every build from the one mark (D20).
font=""
for f in NotoSans-SemiBold NotoSans-Medium NotoSans-Regular 'NotoSans*'; do
    font=$(find "${ROOT}/usr/share/fonts" -name "${f}.ttf" -print -quit)
    [[ -n ${font} ]] && break
done
[[ -n ${font} ]] || die "Noto Sans is not in the image (noto-fonts)"
rsvg-convert -w 128 -h 128 "${WORK}/apps/ui/assets/asimaos-mark.svg" -o "${BUILD}/start-up-mark.png"
magick -background none "${BUILD}/start-up-mark.png" -size 1x22 xc:none +size \
    -fill '#EDF0F5' -font "${font}" -pointsize 34 label:"${ASIMAOS_NAME}" \
    -gravity center -append +repage "${T}/watermark.png"
rm -f "${BUILD}/start-up-mark.png"
# Nix (D28): the "nix profile" commands Asima Dev uses, and flakes, without extra setup.
if [[ -f ${ROOT}/etc/nix/nix.conf ]] && ! grep -q '^experimental-features' "${ROOT}/etc/nix/nix.conf"; then
    printf '\n# Asima OS: the modern nix commands (nix profile, flakes), as Asima Dev uses them.\nexperimental-features = nix-command flakes\n' \
        >> "${ROOT}/etc/nix/nix.conf"
fi
# Baloo indexes file names only, never contents (docs/architecture.md §10).
install -Dm0644 /dev/stdin "${ROOT}/etc/xdg/baloofilerc" <<'EOF'
[Basic Settings]
Indexing-Enabled=false
EOF

step "Programs that no longer get root rights by themselves (at most 13 keep them)"
# Never used on Asima OS; they still work through sudo. lxc-user-nic is only for unprivileged LXC
# containers of ordinary users; Waydroid runs its container as root.
for f in usr/bin/ksu usr/lib/dbus-daemon-launch-helper usr/lib/ssh/ssh-keysign usr/bin/mount.cifs \
         usr/bin/chfn usr/bin/chage usr/bin/gpasswd usr/bin/newgrp usr/lib/lxc/lxc-user-nic; do
    [[ -e ${ROOT}/${f} ]] && chmod u-s "${ROOT}/${f}"
done
mapfile -t SUID < <(find "${ROOT}/usr" -xdev -type f -perm -4000 -printf '/usr/%P\n' | sort)
printf '  root: %s\n' "${SUID[@]}"
find "${ROOT}/usr" -xdev -type f -perm -2000 -printf '  group only: /usr/%P\n' | sort
(( ${#SUID[@]} <= 13 )) || die "${#SUID[@]} programs get root rights by themselves (at most 13 expected)"

step "Disk guard: these tools ask first when they point at the Asima OS disk"
mkdir -p "${ROOT}/usr/local/bin"
for t in rm dd cryptsetup wipefs blkdiscard mkswap mkfs mkfs.btrfs mkfs.ext2 mkfs.ext3 mkfs.ext4 mkfs.fat \
         mkfs.vfat mkfs.msdos mkfs.exfat mkfs.ntfs mkfs.xfs mkfs.f2fs sfdisk sgdisk fdisk cfdisk parted; do
    [[ -x ${ROOT}/usr/bin/${t} ]] && ln -sfn /usr/lib/asimaos/asimaos-guard "${ROOT}/usr/local/bin/${t}"
done
ls "${ROOT}/usr/local/bin" | tr '\n' ' '; echo

# The menu shows every installed app's entry, as stock KDE does (D45).

step "Caches for the added files (MIME types, icons)"
with_root_mounts chroot "${ROOT}" update-mime-database /usr/share/mime
with_root_mounts chroot "${ROOT}" gtk-update-icon-cache -q -t -f /usr/share/icons/hicolor
if [[ -x ${ROOT}/usr/bin/update-desktop-database ]]; then
    with_root_mounts chroot "${ROOT}" update-desktop-database -q /usr/share/applications
fi

echo "Asima OS files: $(find "${DIST}" -type f | wc -l)"
