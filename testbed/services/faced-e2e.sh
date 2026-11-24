#!/usr/bin/env bash
# asimaos-faced end to end on a private D-Bus bus. A still grey picture stands in for the camera:
# enrol one face, then check that it signs in and that a different person doesn't; as a webcam, a
# still picture (a photo) must not get in (two blinks, D47); administrator tokens are one-time; the lock
# screen's Face button (kde-face) is there only while a face is set up.
set -uo pipefail
BIN=${1:-build/services/release/asimaos-faced}
MODELS=${2:-build/models}
T=$(mktemp -d)
fails=0
check() { if eval "$2"; then echo "  ok    $1"; else echo "  FAIL  $1"; fails=$((fails + 1)); fi; }
call() { busctl --user --json=short call io.github.asimaos.Face1 /io/github/asimaos/Face1 io.github.asimaos.Face1 "$@"; }
start() {
    "$BIN" --session --store "$T/store" --models "$MODELS" --pam-file "$T/kde-face" --pam-template "$T/kde-face.template" \
        --picture "$@" & SVC=$!
    for _ in $(seq 100); do busctl --user status io.github.asimaos.Face1 >/dev/null 2>&1 && break; sleep 0.1; done
}
echo "auth required pam_asimaos_face.so use=unlock" > "$T/kde-face.template"
djpeg -grayscale -pnm "$MODELS/lena.jpg" > "$T/a.pgm"
djpeg -grayscale -pnm "$MODELS/messi5.jpg" > "$T/b.pgm"

export DBUS_SESSION_BUS_ADDRESS=$(dbus-daemon --session --fork --print-address)
start "$T/a.pgm"
C=$(busctl --user get-property io.github.asimaos.Face1 /io/github/asimaos/Face1 io.github.asimaos.Face1 Camera)
check "the camera is announced"                 '[[ $C == *"Test picture"* ]]'
call Enroll s "Your face" >/dev/null
for _ in $(seq 300); do [[ -f $T/store/$(id -u).json ]] && break; sleep 0.1; done
check "enrolment stores faceprints, root-only"  '[[ -f $T/store/$(id -u).json && $(stat -c %a $T/store/$(id -u).json) == 600 && $(stat -c %a $T/store) == 700 ]]'
L=$(call ListFaces)
check "the face is listed"                      '[[ $L == *"Your face"* ]]'
check "the lock screen gets its Face button"     '[[ -f $T/kde-face ]]'
V=$(call VerifyUser s "$(id -un)")
check "the same person signs in"                '[[ $V == *true* ]]'
V=$(call VerifyUser s nobody 2>&1)
check "an account without a face steps aside"   '[[ $V == *NoFace* || $V == *"No face"* ]]'
V=$(call VerifyFor ss "$(id -un)" login)
check "infrared: the login screen may use it"   '[[ $V == *true* ]]'
M=$(call AdminMethods s "$(id -un)")
check "the dialog offers the face button"       '[[ $M == *face* ]]'
K=$(call AdminToken ss face "$(id -un)")
TOKEN=$(grep -o 'asimaos-admin-token:[0-9a-f]*' <<<"$K")
check "a face check gives a one-time token"     '[[ -n $TOKEN ]]'
check "…which is valid once for that account"  '[[ $(call ConsumeToken ssu "$(id -un)" "$TOKEN" 0) == *true* && $(call ConsumeToken ssu "$(id -un)" "$TOKEN" 0) == *false* ]]'
K=$(call AdminToken ss face "$(id -un)"); TOKEN=$(grep -o 'asimaos-admin-token:[0-9a-f]*' <<<"$K")
check "…and never for another account"         '[[ $(call ConsumeToken ssu nobody "$TOKEN" 0) == *false* ]]'
check "a made-up token is refused"              '[[ $(call ConsumeToken ssu "$(id -un)" asimaos-admin-token:00 0) == *false* ]]'
kill $SVC; wait $SVC 2>/dev/null

# The same enrolled face, but the camera is a normal webcam: a still picture is a photo.
start "$T/a.pgm" --webcam
K=$(busctl --user get-property io.github.asimaos.Face1 /io/github/asimaos/Face1 io.github.asimaos.Face1 CameraKind)
check "a webcam is announced as one"            '[[ $K == *webcam* ]]'
V=$(call VerifyFor ss "$(id -un)" unlock)
check "webcam: a photo that never blinks fails" '[[ $V == *false* ]]'
V=$(call VerifyFor ss "$(id -un)" login 2>&1)
check "webcam: never at the login screen"       '[[ $V == *NoFace* || $V == *"only unlocks"* ]]'
call VerifyFor ss "$(id -un)" unlock >/dev/null; call VerifyFor ss "$(id -un)" unlock >/dev/null
V=$(call VerifyFor ss "$(id -un)" unlock 2>&1)
check "after three failed tries: the password"  '[[ $V == *"Too many tries"* ]]'
kill $SVC; wait $SVC 2>/dev/null

start "$T/b.pgm"
V=$(call VerifyUser s "$(id -un)")
check "a different person does not sign in"     '[[ $V == *false* ]]'
call DeleteAll >/dev/null
V=$(call VerifyUser s "$(id -un)" 2>&1)
check "after removing, the password is needed"  '[[ ! -e $T/store/$(id -u).json && ( $V == *NoFace* || $V == *"No face"* ) ]]'
check "…and the lock screen's Face button goes"   '[[ ! -e $T/kde-face ]]'
kill $SVC 2>/dev/null
rm -rf "$T"
echo "asimaos-faced: $([[ $fails -eq 0 ]] && echo 'all checks passed' || echo "$fails check(s) failed")"
exit $(( fails > 0 ))
