#!/usr/bin/env bash
# Stage 10: install the pinned Arch package set into a fresh tree and apply Asima OS overlay files.
source /work/builder/stages/common.sh

step "Installing packages from Arch archive ${ASIMAOS_ARCHIVE_DATE}"
rm -rf "${ROOT}"
mkdir -p "${ROOT}/var/lib/pacman" "${ROOT}/var/log" "${ROOT}/etc"
# Download first, with retries: the archive server sometimes stalls on one file, and what was
# already downloaded stays in the shared cache (build/pkgcache), so each try only fetches the rest.
for try in 1 2 3 4 5; do
    if with_root_mounts pacman --root "${ROOT}" --config "${PACMAN_CONF}" \
        --cachedir /var/cache/pacman/pkg -Syw --noconfirm "${ASIMAOS_PACKAGES[@]}"; then
        break
    fi
    [[ ${try} -lt 5 ]] || die "packages could not be downloaded after 5 tries"
    echo "download try ${try} failed; trying again in 20 s"
    sleep 20
done
with_root_mounts pacman --root "${ROOT}" --config "${PACMAN_CONF}" \
    --cachedir /var/cache/pacman/pkg -S --noconfirm "${ASIMAOS_PACKAGES[@]}"
[[ -n "$(kver)" ]] || die "package install failed: no kernel in ${ROOT}/usr/lib/modules"
step "Checking the kernel has what Asima OS is built on"
python3 "${WORK}/builder/check-kernel" "${ROOT}/usr/lib/modules/$(kver)/vmlinuz" || die "the kernel lacks options Asima OS needs"

step "Applying Asima OS overlay files"
cp -a "${WORK}/builder/rootfs/." "${ROOT}/"

# Kernel package hooks may have generated host-specific initramfs files; we build our own.
rm -f "${ROOT}"/boot/initramfs-* "${ROOT}"/boot/vmlinuz-*
