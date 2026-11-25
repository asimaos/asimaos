#!/usr/bin/env bash
# Build the Asima OS apps inside a rootless, pinned Arch container (no sudo needed).
#   apps/build.sh               configure + build all apps into build/apps/
#   apps/build.sh screenshots   build, then render every app screen (light + dark) into apps/screenshots/
#   apps/build.sh e2e           build, then run the installer end to end in dry-run mode (needs an OS build)
#   apps/build.sh services-test build, then test the system services (unit + end-to-end on a private bus)
#   apps/build.sh dist          build, then stage the apps and services for the OS image (build/asimaos-dist)
set -euo pipefail
ROOT="$(dirname "$(dirname "$(readlink -f "$0")")")"
cd "${ROOT}"
source ./builder/asimaos.conf
mkdir -p build      # a fresh checkout has none yet; the container writes its logs and objects there

sed "s|@ARCHIVE_DATE@|${ASIMAOS_ARCHIVE_DATE}|g" builder/pacman.conf.in > builder/pacman.conf
# A local builder/local.conf (not in git) may name faster mirrors in ASIMAOS_CACHE_SERVERS: pacman
# fetches package files there first (CacheServer), while the package lists stay the snapshot's and
# every file is still checked against the snapshot's checksums and signatures; what a mirror no
# longer has comes from the archive.
[[ -f builder/local.conf ]] && source builder/local.conf
for m in ${ASIMAOS_CACHE_SERVERS:-}; do
    sed -i "/^\[\(core\|extra\)\]\$/a CacheServer = ${m}/\$repo/os/\$arch" builder/pacman.conf
done
# ASIMAOS_SLOW_NETWORK=1 (local.conf): pacman waits out a download that stalls, instead of giving up
# after 10 seconds without data.
[[ ${ASIMAOS_SLOW_NETWORK:-} == 1 ]] && sed -i '/^\[options\]$/a DisableDownloadTimeout' builder/pacman.conf
HASH="$(cat apps/Containerfile builder/pacman.conf.in | sha256sum | cut -c1-8)"
IMAGE="localhost/asimaos-apps-builder:${ASIMAOS_ARCHIVE_DATE//\//-}-${HASH}"

if ! podman image exists "${IMAGE}"; then
    echo "==> Building apps builder image ${IMAGE}"
    cache=()
    [[ -d build/pkgcache ]] && cache=(-v "${PWD}/build/pkgcache:/hostcache:ro,z")
    podman build "${cache[@]}" -t "${IMAGE}" -f apps/Containerfile builder/
fi

# The backup tests use restic: the very package the OS image ships, from the pinned archive
# (downloaded into build/pkgcache by an OS build), unpacked into build/tools.
if [[ ${1:-} == services-test && ! -x build/tools/usr/bin/restic ]]; then
    pkg=$(ls build/pkgcache/restic-[0-9]*-x86_64.pkg.tar.zst 2>/dev/null | head -n1 || true)
    if [[ -n ${pkg} ]]; then
        mkdir -p build/tools
        tar -xf "${pkg}" -C build/tools usr/bin/restic
    else
        echo "note: no restic package in build/pkgcache yet: the backup tests are skipped"
    fi
fi

# Compile jobs: one per 1.5 GB of free memory, at most one per core, so a build never runs the
# machine out of memory (C++ with Qt and Rust take about 1 GB a job). ASIMAOS_JOBS=N overrides it.
free_kb=$(awk '/^MemAvailable:/ {print $2}' /proc/meminfo)
jobs=$(( free_kb / 1572864 )); (( jobs < 1 )) && jobs=1; (( jobs > $(nproc) )) && jobs=$(nproc)
jobs=${ASIMAOS_JOBS:-${jobs}}
podman run --rm -e CMAKE_BUILD_PARALLEL_LEVEL="${jobs}" -e CARGO_BUILD_JOBS="${jobs}" \
    -v "${ROOT}:/work:Z" -w /work "${IMAGE}" bash apps/scripts/in-container.sh "$@"
