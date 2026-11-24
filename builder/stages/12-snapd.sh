#!/usr/bin/env bash
# Stage 12: Snap support (D28). Arch doesn't ship snapd; Canonical's own documentation installs it
# on Arch from the AUR, where a snapd developer maintains it. It is built here from a pinned AUR
# revision (whose PKGBUILD pins snapd's release tarball by SHA-256), once per revision, and the
# package is kept in build/aur/ so later builds only install it.
source /work/builder/stages/common.sh

REV=${ASIMAOS_SNAPD_AUR%% *}
CACHE="${BUILD}/aur/snapd-${REV}"
pkg() { ls "${CACHE}"/snapd-*-x86_64.pkg.tar.zst 2>/dev/null | head -n1; }

if [[ -z $(pkg) ]]; then
    step "Building snapd from AUR revision ${REV}"
    # Build tools only live in this throw-away container, never in the image.
    pacman -S --needed --noconfirm --cachedir /var/cache/pacman/pkg \
        base-devel git go go-tools libseccomp libcap systemd xfsprogs python-docutils apparmor \
        autoconf-archive m4 squashfs-tools
    id builder >/dev/null 2>&1 || useradd -m builder
    src=/home/builder/snapd
    rm -rf "${src}"
    runuser -u builder -- git clone -q https://aur.archlinux.org/snapd.git "${src}"
    runuser -u builder -- git -C "${src}" checkout -q "${REV}"
    [[ $(runuser -u builder -- git -C "${src}" rev-parse HEAD) == "${REV}" ]] || die "AUR snapd is not at revision ${REV}"
    (cd "${src}" && runuser -u builder -- env GOFLAGS=-modcacherw makepkg --noconfirm --nodeps)
    mkdir -p "${CACHE}"
    cp "${src}"/snapd-*-x86_64.pkg.tar.zst "${CACHE}/"
    runuser -u builder -- git -C "${src}" log -1 --format="%H %ad %s" --date=short > "${CACHE}/REVISION"
fi
[[ -n $(pkg) ]] || die "snapd was not built"

step "Installing $(basename "$(pkg)")"
with_root_mounts pacman --root "${ROOT}" --config "${PACMAN_CONF}" \
    --cachedir /var/cache/pacman/pkg -U --noconfirm "$(pkg)"
# snap-confine needs these capabilities (snapd.install); EROFS keeps them as extended attributes.
setcap -q - "${ROOT}/usr/lib/snapd/snap-confine" < "${ROOT}/usr/lib/snapd/snap-confine.v2-only.caps"
getcap "${ROOT}/usr/lib/snapd/snap-confine"
