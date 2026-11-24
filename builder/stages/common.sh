# Shared settings for build stages (runs inside the Asima Builder container).
set -euo pipefail

WORK=/work
source "${WORK}/builder/asimaos.conf"

BUILD="${WORK}/build"
ROOT="${BUILD}/rootfs"        # assembled OS tree
STAGE="${BUILD}/staging"      # partition contents (esp/, pool/)
ENGINE="${BUILD}/services/release/asimaos-engine"   # asimaos-install, asimaos-repair and Rescue's engine
OUT="${BUILD}/out"            # release artifacts
PACMAN_CONF="${BUILD}/pacman.conf"
KEYS="${BUILD}/keys"          # this build's own keys (D25), never in the repository
SB_KEY="${KEYS}/secureboot.key"
SB_CERT="${KEYS}/secureboot.crt"
SHIM="${BUILD}/shim"          # Fedora's Microsoft-signed shim (scripts/fetch-shim.sh), if present

# sign_efi IN OUT: an EFI program signed with this build's Secure Boot key.
sign_efi() {
    /usr/lib/systemd/systemd-sbsign sign --private-key="${SB_KEY}" --certificate="${SB_CERT}" --output="$2" "$1" >/dev/null
}

V="${ASIMAOS_VERSION}"

kver() { ls "${ROOT}/usr/lib/modules" | head -n1; }
step() { printf '\n\033[1;31m==> %s\033[0m\n' "$*"; }
die()  { printf '\033[1;31mFATAL: %s\033[0m\n' "$*" >&2; exit 1; }

# Run a command with /proc, /sys, /dev, /run, /tmp available inside ${ROOT}, in a private
# mount namespace (cleaned up automatically). Replaces pacstrap/arch-chroot, which need
# devtmpfs or subuid maps that a rootless container cannot provide.
#   with_root_mounts pacman --root "${ROOT}" ...     (runs in the builder, sees prepared tree)
#   with_root_mounts chroot "${ROOT}" mkinitcpio ... (runs inside the image tree)
with_root_mounts() {
    unshare --mount --fork bash -euo pipefail -c '
        r="$1"; shift
        mount --make-rprivate /
        mount --bind "$r" "$r"
        mkdir -p "$r/proc" "$r/sys" "$r/dev" "$r/run" "$r/tmp"
        mount -t proc proc "$r/proc"
        mount --rbind /sys "$r/sys"
        mount --rbind /dev "$r/dev"
        mount -t tmpfs tmpfs "$r/run"
        mount -t tmpfs tmpfs "$r/tmp"
        exec "$@"
    ' _ "${ROOT}" "$@"
}

# Size limits (asimaos.conf): fails the build when FILE outgrew LIMIT MiB.
check_size() {  # FILE LIMIT_MB WHAT
    local mb=$(( $(stat -c %s "$1") / 1048576 ))
    [[ ${mb} -le $2 ]] || die "$3 is ${mb} MiB, over the ${2} MiB limit in asimaos.conf (raise it on purpose, or slim the image)"
    echo "$3: ${mb} MiB (limit $2 MiB)"
}

# Signed checksums of what is published: SHA256SUMS of the ISO and the update files, its
# Ed25519 signature (SHA256SUMS.sig, the release key) and the key as PEM with its fingerprint, so
# anyone can check a download, with aosupd or with OpenSSL (docs/guides/check-a-download.md).
sign_outputs() {
    local aosupd=${BUILD}/asimaos-dist/usr/bin/aosupd
    # Whatever of this version exists yet (the ISO stage runs before the update file is made);
    # never an older build's files that are still lying in build/out.
    (cd "${OUT}" && shopt -s nullglob && files=(asimaos-"${ASIMAOS_VERSION}"-*.iso asimaos-"${ASIMAOS_VERSION}"-*.aosupd) \
        && if ((${#files[@]})); then sha256sum "${files[@]}" > SHA256SUMS; fi)
    [[ -s ${OUT}/SHA256SUMS ]] || return 0
    "${aosupd}" sign --key "${BUILD}/keys/release.key" "${OUT}/SHA256SUMS" >/dev/null
    "${aosupd}" verify-file --key "${BUILD}/keys/release.pub" "${OUT}/SHA256SUMS"
    "${aosupd}" key-info --key "${BUILD}/keys/release.pub" > "${OUT}/asimaos-release-key.txt"
    sed -n '/BEGIN/,/END/p' "${OUT}/asimaos-release-key.txt" > "${OUT}/asimaos-release-key.pem"
    grep Fingerprint "${OUT}/asimaos-release-key.txt"
}

mkdir -p "${BUILD}" "${OUT}"
