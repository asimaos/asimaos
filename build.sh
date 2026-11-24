#!/usr/bin/env bash
# Asima Builder (host entry point). Runs every build stage inside a rootless Arch container.
#   ./build.sh            run all stages
#   ./build.sh 40 50 60   run only the given stages
set -euo pipefail
cd "$(dirname "$(readlink -f "$0")")"
source ./builder/asimaos.conf

BUILDER_HASH="$(cat builder/Containerfile builder/pacman.conf.in | sha256sum | cut -c1-8)"
IMAGE="localhost/asimaos-builder:${ASIMAOS_ARCHIVE_DATE//\//-}-${BUILDER_HASH}"

mkdir -p build/pkgcache
sed "s|@ARCHIVE_DATE@|${ASIMAOS_ARCHIVE_DATE}|g" builder/pacman.conf.in > builder/pacman.conf
cp builder/pacman.conf build/pacman.conf

if ! podman image exists "${IMAGE}"; then
    echo "==> Building builder image ${IMAGE}"
    podman build -t "${IMAGE}" builder/
fi

# ASIMAOS_OFFLINE=1: when the Arch archive can't be reached, build from the packages an earlier
# build of the same snapshot downloaded (build/pkgcache, each with its signature): a local
# repository is made from them and used instead of the archive. pacman still checks every
# package's signature against Arch's keys; a package the cache lacks stops the build.
if [[ ${ASIMAOS_OFFLINE:-} == 1 ]]; then
    echo "==> Offline: a local repository from build/pkgcache (snapshot ${ASIMAOS_ARCHIVE_DATE})"
    rm -rf build/offline-repo && mkdir -p build/offline-repo
    podman run --rm -v "${PWD}:/work:Z" -w /work/build/offline-repo "${IMAGE}" bash -c \
        'ln -s ../pkgcache/*.pkg.tar.zst ../pkgcache/*.pkg.tar.zst.sig . && repo-add -q offline.db.tar.zst *.pkg.tar.zst'
    sed -i -e '/^\[core\]$/,$d' build/pacman.conf
    printf '[offline]\nServer = file:///work/build/offline-repo\n' >> build/pacman.conf
fi

# Fedora's Microsoft-signed shim for Secure Boot (host side: needs dnf).
builder/fetch-shim.sh

if [[ $# -gt 0 ]]; then
    STAGES=()
    for n in "$@"; do STAGES+=(builder/stages/"${n}"-*.sh); done
else
    STAGES=(builder/stages/[0-9][0-9]-*.sh)
fi

TIMES=()
for stage in "${STAGES[@]}"; do
    [[ -f "${stage}" ]] || { echo "unknown stage: ${stage}" >&2; exit 1; }
    started=${SECONDS}
    # The update tests' later versions pass these through; unset, they stay unset.
    podman run --rm --privileged \
        -e ASIMAOS_VERSION_OVERRIDE -e ASIMAOS_NOTES_EXTRA -e ASIMAOS_TEST_BREAK \
        -v "${PWD}:/work:Z" \
        -v "${PWD}/build/pkgcache:/var/cache/pacman/pkg:Z" \
        "${IMAGE}" bash "/work/${stage}"
    TIMES+=("$(printf '%-18s %3d min %02d s' "$(basename "${stage}" .sh)" $(( (SECONDS - started) / 60 )) $(( (SECONDS - started) % 60 )))")
done

echo
echo "==> Time per stage:"
printf '    %s\n' "${TIMES[@]}"

echo
echo "==> Done. Artifacts in build/out/:"
ls -lh build/out/
