#!/usr/bin/env bash
# Runs inside the apps builder container (see apps/build.sh).
#   in-container.sh                       build
#   in-container.sh screenshots [APP]     build, then render every screen (light + dark)
#   in-container.sh e2e                   build, then run the installer end to end (dry run):
#                                         GUI → real engine (JSON, secrets on stdin) → GUI
#   in-container.sh services-test         build, then the services' unit tests and end-to-end tests
#   in-container.sh translations          collect new strings into apps/i18n/*.ts
#   in-container.sh qmllint               build, then Qt's QML checker over every app (must stay clean)
#   in-container.sh dist                  build, then stage everything Asima OS adds to the OS image
#                                         in build/asimaos-dist (copied into the image by ./build.sh)
set -euo pipefail
cd /work
BUILD=build/apps

cmake -S apps -B "${BUILD}" -G Ninja -DCMAKE_BUILD_TYPE=Release > build/apps-configure.log 2>&1 \
    || { cat build/apps-configure.log; exit 1; }
# Translations: CMake makes empty files at configure time; the strings are collected from
# the sources once here, and `apps/build.sh translations` collects new ones later.
grep -q '<message' apps/i18n/asimaos_bn.ts 2>/dev/null || cmake --build "${BUILD}" --target update_translations > /dev/null
cmake --build "${BUILD}"

# System services (Rust): crates are cached in build/cargo, objects in build/services. The face
# models are downloaded once (pinned SHA-256) into build/models; the PAM module is plain C.
export CARGO_HOME=/work/build/cargo
cargo build --release --quiet --manifest-path services/Cargo.toml --target-dir build/services
bash services/asimaos-faced/models/fetch.sh build/models
gcc -shared -fPIC -O2 -Wall -Wextra -o build/services/pam_asimaos_face.so services/pam/pam_asimaos_face.c -lpam -lsystemd

MODE="${1:-}"
if [[ ${MODE} == qmllint ]]; then
    # Warnings fail too: the apps are kept free of them.
    out=$(cmake --build "${BUILD}" --target all_qmllint 2>&1) || { echo "${out}"; exit 1; }
    if grep -qE '^(Warning|Error):' <<<"${out}"; then echo "${out}" | grep -A2 -E '^(Warning|Error):'; exit 1; fi
    echo "qmllint: every app is clean"
    exit 0
fi
if [[ ${MODE} == translations ]]; then
    cmake --build "${BUILD}" --target update_translations
    exit 0
fi
if [[ ${MODE} == dist ]]; then
    D=/work/build/asimaos-dist
    rm -rf "${D}"
    DESTDIR="${D}" cmake --install "${BUILD}" --prefix /usr > /dev/null
    # The installer only runs in the live session; early mount puts its entry in place there.
    mkdir -p "${D}/usr/share/asimaos/live"
    mv "${D}/usr/share/applications/io.github.asimaos.Installer.desktop" "${D}/usr/share/asimaos/live/"
    for s in asimaos-updaterd asimaos-maintenanced asimaos-managerd asimaos-gamingd asimaos-faced; do
        install -Dm0755 "build/services/release/${s}" "${D}/usr/lib/asimaos/${s}"
    done
    install -Dm0755 build/services/release/aosupd "${D}/usr/bin/aosupd"
    install -Dm0755 build/services/pam_asimaos_face.so "${D}/usr/lib/security/pam_asimaos_face.so"
    install -Dm0644 -t "${D}/usr/share/asimaos/face" build/models/face_detection_yunet_2023mar.onnx \
        build/models/face_recognition_sface_2021dec.onnx
    install -Dm0644 -t "${D}/usr/share/dbus-1/system.d" services/data/dbus/*.conf
    install -Dm0644 -t "${D}/usr/share/dbus-1/system-services" services/data/dbus/*.service
    install -Dm0644 -t "${D}/usr/lib/systemd/system" services/data/systemd/*.service services/data/systemd/*.timer
    install -Dm0644 -t "${D}/usr/share/polkit-1/actions" services/data/polkit/io.github.asimaos.policy
    install -Dm0644 -t "${D}/usr/share/mime/packages" services/data/mime/asimaos-update.xml
    install -Dm0644 -t "${D}/usr/lib/udev/rules.d" services/data/udev/*.rules
    # The services' and apps' design and contracts, named by each unit's Documentation= line.
    install -Dm0644 apps/DESIGN.md "${D}/usr/share/doc/asimaos/DESIGN.md"
    # Each app its own icon (apps/ui/assets/make-app-icons.py); the installer and update files the mark.
    for icon in Updater:updater Maintenance:maintenance Manager:manager Gaming:gaming Dev:dev AppCenter:appcenter; do
        install -Dm0644 "apps/ui/assets/apps/${icon#*:}.svg" "${D}/usr/share/icons/hicolor/scalable/apps/io.github.asimaos.${icon%%:*}.svg"
    done
    install -Dm0644 apps/ui/assets/asimaos-mark.svg "${D}/usr/share/icons/hicolor/scalable/apps/io.github.asimaos.Installer.svg"
    install -Dm0644 apps/ui/assets/asimaos-mark.svg "${D}/usr/share/icons/hicolor/scalable/apps/asimaos.svg"   # os-release LOGO
    install -Dm0644 apps/ui/assets/asimaos-mark.svg "${D}/usr/share/icons/hicolor/scalable/mimetypes/application-x-asimaos-update.svg"
    echo "staged in build/asimaos-dist: $(find "${D}" -type f | wc -l) files, $(du -sh "${D}" | cut -f1)"
    exit 0
fi
if [[ ${MODE} == services-test ]]; then
    # restic for the backup tests: the package the OS image ships, unpacked into build/tools by
    # apps/build.sh.
    export PATH=/work/build/tools/usr/bin:${PATH}
    ASIMAOS_FACE_MODELS=/work/build/models cargo test --release --quiet --manifest-path services/Cargo.toml \
        --target-dir build/services 2>&1 | grep -E 'test result|FAILED|panicked' || true
    rc=0
    for s in updaterd maintenanced managerd gamingd; do
        bash "testbed/services/${s}-e2e.sh" "build/services/release/asimaos-${s}" || rc=1
    done
    bash testbed/services/faced-e2e.sh build/services/release/asimaos-faced build/models || rc=1
    for app in updater maintenance manager gaming; do
        bash "testbed/services/${app}-gui-e2e.sh" "build/apps/asimaos-${app}/asimaos-${app}" \
            "build/services/release/asimaos-${app}d" build/gui-e2e || rc=1
    done
    exit ${rc}
fi
[[ ${MODE} == screenshots || ${MODE} == e2e ]] || exit 0
ONLY="${2:-}"

export XDG_RUNTIME_DIR=/tmp/xdg-runtime && mkdir -p -m 0700 "${XDG_RUNTIME_DIR}"
export LIBGL_ALWAYS_SOFTWARE=1 QT_QPA_PLATFORM=xcb DISPLAY=:99
Xvfb :99 -screen 0 1920x1200x24 -nolisten tcp >/dev/null 2>&1 &
XVFB=$!
trap 'kill ${XVFB} 2>/dev/null || true' EXIT
sleep 1

if [[ ${MODE} == e2e ]]; then
    # Needs the installer files of an OS build (build/iso/asimaos) and its factory /etc
    # (build/rootfs/usr/share/factory/etc). No disk is touched: a sparse file stands in.
    [[ -d build/iso/asimaos && -f build/rootfs/usr/share/factory/etc/passwd ]] \
        || { echo "e2e: needs an OS build first (build/iso/asimaos, build/rootfs)"; exit 1; }
    # Sparse: the installer needs a disk of at least 48 GB.
    rm -rf /tmp/asimaos-install.* && truncate -s 64G /tmp/e2e-disk.img
    ASIMAOS_INSTALL_ENGINE=engines/asimaos-install ASIMAOS_ENGINE=build/services/release/asimaos-engine \
    ASIMAOS_INSTALL_SOURCE=build/iso/asimaos \
    ASIMAOS_INSTALL_FACTORY_ETC=build/rootfs/usr/share/factory/etc ASIMAOS_INSTALL_KEEP_WORK=1 \
        "${BUILD}/asimaos-installer/asimaos-installer" --dry-run --autotest-target /tmp/e2e-disk.img \
        --size 1320x860 --screenshot /tmp/e2e-done.png --delay 9000 2>&1 \
        | grep -vE 'propagateSizeHints|XDG_RUNTIME_DIR' || true
    W=$(ls -d /tmp/asimaos-install.* 2>/dev/null | head -n1)
    U=${W}/pool/@data/overlay/etc/upper
    fails=0
    check() { if eval "$2"; then echo "  ok    $1"; else echo "  FAIL  $1"; fails=$((fails + 1)); fi; }
    check "engine staged a system"            '[[ -n ${W} && -d ${U} ]]'
    check "account in passwd (uid 1000)"      'grep -q "^tester:x:1000:1000:Integration Test:/home/tester:/bin/bash$" "${U}/passwd"'
    check "account is an administrator"       'grep -qE "^wheel:[^:]*:[^:]*:(.*,)?tester$" "${U}/group"'
    check "password hashed, not plain"        'grep -qF "tester:\$6\$" "${U}/shadow" && ! grep -q integration-test-pw "${U}/shadow"'
    check "shadow is root-only (0600)"        '[[ $(stat -c %a "${U}/shadow") == 600 ]]'
    check "root locked (the account is admin)" 'grep -q "^root:!\*:" "${U}/shadow"'
    check "computer name"                     '[[ $(cat "${U}/hostname") == tester-pc ]]'
    check "install.conf says encrypted"       'grep -qx ENCRYPTED=yes "${U}/asimaos/install.conf"'
    check "private home owned by the user"    '[[ $(stat -c "%u %a" "${W}/pool/@home/tester") == "1000 700" ]]'
    check "GUI reached the Done page"         '[[ -s /tmp/e2e-done.png ]]'
    exit $(( fails > 0 ))
fi

OUT=apps/screenshots
mkdir -p "${OUT}"

# shoot APP PAGE[:STATE][=NAME][@WxH] THEME → apps/screenshots/APP-PAGE[-NAME or last part of STATE]-THEME.png
# (@WxH: a taller window for a long page; 1320x860 otherwise)
shoot() {
    local app=$1 shot=$2 theme=$3 page state name suffix="" size=1320x860
    if [[ "${shot}" == *@* ]]; then size="${shot##*@}"; shot="${shot%@*}"; fi
    if [[ "${shot}" == *=* ]]; then suffix="${shot#*=}"; shot="${shot%%=*}"; fi
    page="${shot%%:*}"
    state=""
    [[ "${shot}" == *:* ]] && state="${shot#*:}"
    [[ -z ${suffix} && -n ${state} ]] && suffix="${state##*-}"
    name="${app}-${page}${suffix:+-${suffix}}-${theme}.png"
    "${BUILD}/${app}/${app}" --demo --theme "${theme}" --page "${page}" \
        ${state:+--state "${state}"} --size "${size}" --screenshot "${OUT}/${name}" 2>&1 \
        | grep -vE 'propagateSizeHints|XDG_RUNTIME_DIR' || true
    echo "  ${name}"
}

declare -A SHOTS=(
    [asimaos-updater]="updates updates:update-ready updates:update-installing updates:update-done updates:update-error versions"
    [asimaos-maintenance]="overview overview:windows-first health health:health-report backups backups:backup-tab backups:backup-restore backups:backup-setup security security:security-setup@1320x1600
                          security:security-pending security:security-plain security:security-pin@1320x1600 security:security-keys security:security-retiring security:security-ownkeys security:security-ownkeys-setup=ownkeys-setup@1320x1300 changes reset@1320x1900 reset:reset-key"
    [asimaos-manager]="home@1320x1010 power power:ideapad display display:laptop biometrics biometrics:enrolling hardware shield home:laptop@1320x1110
                      biometrics:laptop biometrics:laptop-face biometrics:webcam"
    [asimaos-gaming]="gaming controllers handheld@1320x1500 lighting gaming:laptop"
    [asimaos-dev]="containers machines machines:gpu storage"
    [asimaos-appcenter]="store store:search store:busy installed updates file:file"
    [asimaos-installer]="welcome language disk account:filled encryption:filled encryption:filled-plain
                        summary:filled install:installing install:error done:done done:done-filled-plain
                        repair-disk:repair-disks repair-choice:repair-probed repair-run:repair-running repair-run:repair-done
                        repair-run:repair-error"
    [asimaos-rescue]="home unlock disk:unlocked restore:unlocked reinstall:unlocked keys reset:unlocked wipe
                     job:check-running=check-running job:check-done=check-done job:approve-key-done=key-done
                     job:factory-reset-done=reset-done job:wipe-running=wipe-running"
    [asimaos-polkit-agent]="dialog dialog:fingerprint dialog:error"
)

for theme in light dark; do
    for app in asimaos-updater asimaos-maintenance asimaos-manager asimaos-gaming asimaos-dev asimaos-appcenter asimaos-installer asimaos-rescue \
               asimaos-polkit-agent; do
        [[ -z ${ONLY} || ${ONLY} == "${app}" ]] || continue
        for shot in ${SHOTS[${app}]}; do shoot "${app}" "${shot}" "${theme}"; done
    done
done
