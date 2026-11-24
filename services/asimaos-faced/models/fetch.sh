#!/usr/bin/env bash
# Downloads the two open face models Asima OS uses, pinned by SHA-256 (OpenCV Zoo):
#   YuNet  face detection    (MIT)        face_detection_yunet_2023mar.onnx
#   SFace  face recognition  (Apache-2.0) face_recognition_sface_2021dec.onnx
# and two sample photos of different people from OpenCV's sample data, only for the face tests
# (the unit test in src/face.rs and testbed/services/faced-e2e.sh). The photos never go into the
# system image: apps/scripts/in-container.sh installs only the two models, by name.
#   fetch.sh DIR
set -euo pipefail
DIR=${1:?usage: fetch.sh DIR}
mkdir -p "${DIR}"

# fetch BASE-URL, then "SHA-256 PATH" lines on standard input
fetch() {
    local base=$1 sha path f
    while read -r sha path; do
        f=${DIR}/${path##*/}
        if [[ ! -f ${f} ]] || ! echo "${sha}  ${f}" | sha256sum -c --quiet 2>/dev/null; then
            curl -fsSL -o "${f}.part" "${base}/${path}"
            echo "${sha}  ${f}.part" | sha256sum -c --quiet || { rm -f "${f}.part"; echo "checksum mismatch: ${path}" >&2; exit 1; }
            mv "${f}.part" "${f}"
        fi
    done
}

fetch https://media.githubusercontent.com/media/opencv/opencv_zoo/main/models <<'MODELS'
8f2383e4dd3cfbb4553ea8718107fc0423210dc964f9f4280604804ed2552fa4 face_detection_yunet/face_detection_yunet_2023mar.onnx
0ba9fbfa01b5270c96627c4ef784da859931e02f04419c829e83484087c34e79 face_recognition_sface/face_recognition_sface_2021dec.onnx
MODELS

fetch https://raw.githubusercontent.com/opencv/opencv/4.x/samples/data <<'PHOTOS'
7de7ed51a1594fff247f4cae2301eceacf5313d6011e37b4a4c8733f7bb72c07 lena.jpg
1d570e49654e84c7a943918537bd9e5e1ef82920152e147c834006e235be97c9 messi5.jpg
PHOTOS
