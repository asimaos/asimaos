#!/usr/bin/env bash
# Runs on the build host (not in the builder): Fedora's shim, signed by Microsoft (D24), so Asima OS
# OS starts with Secure Boot on any PC that trusts Microsoft's UEFI CA. Needs dnf (a Fedora host);
# the RPM's signature is checked against the host's Fedora keys before anything is used.
# Result: build/shim/shimx64.efi and build/shim/mmx64.efi (MokManager). Without them the build
# still works: systemd-boot then starts directly (signed with this build's key only).
set -euo pipefail
cd "$(dirname "$(readlink -f "$0")")/.."
[[ -f build/shim/shimx64.efi && -f build/shim/mmx64.efi ]] && exit 0
if ! command -v dnf >/dev/null || ! command -v rpm2cpio >/dev/null; then
    echo "note: no dnf on this host: building without shim (Secure Boot needs this build's key in the firmware)"
    exit 0
fi
dl=$(mktemp -d build/shim-download.XXXXXX)
trap 'rm -rf "${dl}"' EXIT
dnf download -q shim-x64 --destdir "${dl}"
rpm=$(ls "${dl}"/shim-x64-*.x86_64.rpm)
rpm -K "${rpm}" | grep -q 'digests signatures OK' || { echo "shim: the RPM's signature is not valid" >&2; exit 1; }
(cd "${dl}" && rpm2cpio "${rpm##*/}" | cpio -idm --quiet './usr/lib/efi/shim/*/EFI/fedora/shimx64.efi' './usr/lib/efi/shim/*/EFI/fedora/mmx64.efi')
mkdir -p build/shim
cp "${dl}"/usr/lib/efi/shim/*/EFI/fedora/shimx64.efi "${dl}"/usr/lib/efi/shim/*/EFI/fedora/mmx64.efi build/shim/
rpm -qp --qf '%{NAME}-%{VERSION}-%{RELEASE}\n' "${rpm}" > build/shim/VERSION
echo "shim: $(cat build/shim/VERSION) (Microsoft-signed, from Fedora)"
