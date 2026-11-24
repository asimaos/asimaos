#!/usr/bin/env bash
# Stage 30: make the tree hermetic: stamp identity, move /etc into /usr/share/factory/etc, strip pacman.
source /work/builder/stages/common.sh

step "Setting TEST-ONLY root password"
echo "root:${ASIMAOS_TEST_ROOT_PASSWORD}" | with_root_mounts chroot "${ROOT}" chpasswd

step "Stamping /usr/lib/os-release (IMAGE_VERSION=${V})"
cat > "${ROOT}/usr/lib/os-release" <<EOF
NAME="Asima OS"
PRETTY_NAME="Asima OS"
VERSION="${V} (${ASIMAOS_RELEASE})"
ID=asimaos
ID_LIKE=arch
VERSION_ID=${V}
IMAGE_ID=asimaos
IMAGE_VERSION=${V}
BUILD_ID=arch-archive-${ASIMAOS_ARCHIVE_DATE//\//-}
ANSI_COLOR="0;34"
LOGO=asimaos
EOF
ln -sf ../usr/lib/os-release "${ROOT}/etc/os-release"

# The update tests only: a version whose login screen never starts, to prove that a broken update
# goes back to the previous version by itself. Undone again by the next normal build.
UNIT="${ROOT}/usr/lib/systemd/system/plasmalogin.service"
if [[ ${ASIMAOS_TEST_BREAK:-} == login ]]; then
    step "TEST BUILD: the login screen is broken on purpose (automatic fallback test)"
    [[ -L ${UNIT} ]] || mv "${UNIT}" "${BUILD}/plasmalogin.service.orig"
    ln -sfn /dev/null "${UNIT}"
elif [[ -L ${UNIT} && -f ${BUILD}/plasmalogin.service.orig ]]; then
    mv -f "${BUILD}/plasmalogin.service.orig" "${UNIT}"
fi

step "Removing machine identity (generated on first boot)"
rm -f "${ROOT}/etc/machine-id"

step "Default timezone UTC (installer will set the real one)"
ln -sf ../usr/share/zoneinfo/UTC "${ROOT}/etc/localtime"

step "The package list with licences and where their sources are"
# Every program in the image, its version, licences and upstream, and where its exact source is:
# Arch's packaging repository at the matching tag, and Arch's source archive. Kept in the image
# (/usr/share/asimaos/licences), next to the outputs, and on the ISO. The licence texts themselves
# are in /usr/share/licenses.
LIC=${ROOT}/usr/share/asimaos/licences
install -d "${LIC}"
# Stage 30 run again (the update tests' versions, docs/building.md) finds pacman's database gone:
# the list from the first run stays.
if [[ -d ${ROOT}/var/lib/pacman/local ]]; then
{
    printf 'name\tversion\tbase\tlicences\tupstream\tpackaging (PKGBUILD at this tag)\n'
    for desc in "${ROOT}"/var/lib/pacman/local/*/desc; do
        awk '
            /^%[A-Z]+%$/ { key = $0; next }
            NF == 0 { key = ""; next }
            key == "%NAME%" { name = $0 } key == "%VERSION%" { ver = $0 } key == "%BASE%" { base = $0 }
            key == "%URL%" { url = $0 } key == "%LICENSE%" { lic = lic (lic ? " " : "") $0 }
            END {
                if (base == "") base = name
                tag = ver; gsub(":", "-", tag)
                printf "%s\t%s\t%s\t%s\t%s\thttps://gitlab.archlinux.org/archlinux/packaging/packages/%s/-/tree/%s\n", name, ver, base, lic, url, base, tag
            }' "${desc}"
    done | sort
} > "${LIC}/packages.tsv"
else
    [[ $(wc -l < "${LIC}/packages.tsv" 2>/dev/null || echo 0) -gt 1 ]] \
        || die "no package list and no pacman database: build from stage 10 again"
fi
cat > "${LIC}/SOURCES.md" <<EOF
# Asima OS ${V}: programs and their sources

\`packages.tsv\` lists every package in this version of Asima OS: its version, its licences, its
upstream project and the Arch Linux packaging (PKGBUILD) that built it. All packages come from the
Arch Linux Archive snapshot of ${ASIMAOS_ARCHIVE_DATE} (https://archive.archlinux.org/repos/${ASIMAOS_ARCHIVE_DATE}/).

The source code of each package is available:

- from Arch Linux's source archive, https://sources.archlinux.org/, for the version listed, and
- as its build recipe from the packaging link in the list, which names the upstream sources.

The Asima OS parts (the apps, the services, the start-up scripts) are GPL-3.0-or-later, with their
source at https://github.com/asimaos/asimaos. On request, the Asima OS project sends the
complete corresponding source of any GPL program in this version for three years from its release.
The licence texts are in /usr/share/licenses.
EOF
cp "${LIC}/packages.tsv" "${OUT}/asimaos-${V}-packages.tsv"
echo "$(($(wc -l < "${LIC}/packages.tsv") - 1)) packages listed"

step "Stripping pacman from the runtime image"
if [[ -d "${ROOT}/var/lib/pacman/local" ]]; then
    # pacman is a HoldPkg (protected); use a config without HoldPkg for this one removal.
    grep -v '^HoldPkg' "${PACMAN_CONF}" > "${BUILD}/pacman-nohold.conf"
    pacman --root "${ROOT}" --config "${BUILD}/pacman-nohold.conf" -Rdd --noconfirm pacman
fi
rm -rf "${ROOT}/var/lib/pacman" "${ROOT}/etc/pacman.d"

step "Folders under /var at every start"
# An installed system's /var is its own subvolume (@var) and starts empty, so the folders that
# packages put under /var in this image never reach it (snapd's AppArmor folder, for example).
# Every one of them is listed for systemd-tmpfiles, which makes the missing ones at every start
# with the image's owner and mode (numeric: names would come from the builder); ":" keeps
# whatever an existing folder has now.
{
    echo "# Asima OS: the folders the image has under /var (made by builder/stages/30-finalize.sh)."
    find "${ROOT}/var" -mindepth 1 -type d \( -path "${ROOT}/var/cache" -o -path "${ROOT}/var/tmp" \) -prune -o \
        -type d -printf '%P %m %U %G\n' | sort | while read -r path mode user group; do
        printf 'd /var/%s :%04o :%s :%s -\n' "${path}" "$((8#${mode}))" "${user}" "${group}"
    done
} > "${ROOT}/usr/lib/tmpfiles.d/asimaos-var.conf"
grep -c '^d ' "${ROOT}/usr/lib/tmpfiles.d/asimaos-var.conf"
grep -q ' /var/lib/snapd/apparmor/snap-confine ' "${ROOT}/usr/lib/tmpfiles.d/asimaos-var.conf" \
    || die "snapd's AppArmor folder is missing from asimaos-var.conf"

step "Relocating /etc to /usr/share/factory/etc"
mkdir -p "${ROOT}/usr/share/factory/etc"
cp -a "${ROOT}/etc/." "${ROOT}/usr/share/factory/etc/"
# Checksums of the factory files: after an update, files whose default changed while
# the person had changed them are listed in Asima Maintenance (asimaos-updaterd --finish-update).
(cd "${ROOT}/usr/share/factory/etc" && find . -type f -print0 | sort -z | xargs -0 sha256sum) \
    > "${ROOT}/usr/share/factory/etc.sha256"
