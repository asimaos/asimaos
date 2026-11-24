#!/usr/bin/env bash
# Stage 40: seal /usr into a 4 KB EROFS image + dm-verity hash tree.
source /work/builder/stages/common.sh

IMG="${OUT}/usr_${V}.erofs"
HASH="${OUT}/verity_${V}.img"

step "Building EROFS image of /usr (LZ4HC: about half the size, still fast to read)"
rm -f "${IMG}" "${HASH}"
# -T0: fixed timestamps, so the same tree gives the same image.
mkfs.erofs -zlz4hc,12 -C65536 -Eztailpacking -b 4096 -T0 "${IMG}" "${ROOT}/usr"

step "Generating dm-verity hash tree"
veritysetup format --data-block-size=4096 --hash-block-size=4096 \
    --root-hash-file="${OUT}/usrhash_${V}.txt" "${IMG}" "${HASH}"
echo "usrhash: $(cat "${OUT}/usrhash_${V}.txt")"
ls -lh "${IMG}" "${HASH}"
check_size "${IMG}" "${ASIMAOS_LIMIT_USR_MB}" "The /usr image"
