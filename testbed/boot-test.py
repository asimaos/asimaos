#!/usr/bin/env python3
"""Asima OS boot test (QEMU + OVMF, driven over the serial console).

  testbed/boot-test.py              boot build/out/asimaos-disk.raw (throwaway copy), verify, reboot, verify
  testbed/boot-test.py --iso        boot the ISO (live session checks), run asimaos-install onto a blank
                                  disk (encrypted, with a test account), then boot the installed
                                  disk and run the same checks
  testbed/boot-test.py --iso --plain   the same, installed without encryption (decision D9)
  testbed/boot-test.py --iso --update V2 V3   install, then update to V2 (a good update),
                                  go back to the backup once and for good, then update to V3 (a
                                  deliberately broken update) and check that the computer goes
                                  back to V2 by itself (build/out/asimaos-V-x86_64.aosupd)
  testbed/boot-test.py --menu-shot  a picture of the start menu of the disk image
  testbed/boot-test.py --secureboot the disk-image test with Secure Boot enforced: OVMF with
                                  Microsoft's keys, plus this build's certificate in db (added
                                  with virt-fw-vars from build/tools/virt-fw); shim → systemd-boot
                                  → the UKI must all pass the firmware's checks
  testbed/boot-test.py --reset      Reset settings: /etc and a person's settings go back to the
                                  defaults at the restart, the accounts, files and keyboard stay;
                                  then the factory reset: a fresh account and new disk secrets are
                                  staged in Asima Maintenance; after the restart nothing of the old
                                  system is left, the version is the same, the old passphrase fails
  testbed/boot-test.py --own-keys   own Secure Boot keys: the firmware's lists are saved and this
                                  computer's keys made; in Setup Mode they are written (Microsoft's
                                  kept, the Asima OS key added); it starts again with Secure Boot on
  testbed/boot-test.py --rescue     Asima Rescue: it unlocks the disk and reinstalls the
                                  running version from its update file on a USB drive
  testbed/boot-test.py --flathub    an app installs from Flathub (the VM needs the internet)
  testbed/boot-test.py --repair     "Repair this computer": an account is deleted after a
                                  snapshot, the ESP is formatted; asimaos-repair on the USB stick
                                  brings back the start menu and the account, and it starts again
  testbed/boot-test.py --trust      a PC that trusts only Microsoft's 2023 UEFI certificate, with the
                                  Asima OS key approved as a MOK (not in db) and a TPM 2.0: the
                                  dual-signed shim starts, lockdown is on, the signed NVIDIA
                                  drivers pass the kernel's check, the start-up keyboard layout
                                  follows the system's, TPM unlock works without the passphrase,
                                  and Asima Rescue can't unseal the disk key (the
                                  systemd-pcrlock policy, PCR 4 + 7)

Installed systems have root locked (the installer's account is the administrator), so the test
signs in as that account and uses sudo. The disk image and the live ISO still have the TEST-ONLY
root password from asimaos.conf.

The LUKS passphrase is passed as a systemd credential via SMBIOS (VM-only mechanism),
so the test needs no access to the on-screen prompt.
"""
import os
import re
import select
import shutil
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BUILD = os.path.join(HERE, "build")
PASSPHRASE = "asimaos"                     # disk image built by the builder
INSTALL_PASSPHRASE = "asimaos-install-test"  # asimaos-install requires >= 8 characters
TEST_USER, TEST_PASSWORD = "tester", "tester-pw"   # account created by asimaos-install (TEST ONLY)
ROOT_PASSWORD = "asimaos"
# CSI sequences, OSC sequences (systemd 262 emits OSC 3008 context markers), charset selects, CR
ANSI = re.compile(rb"\x1b\[[0-9;?]*[A-Za-z]|\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)|\x1b[()][A-Z0-9]|\r")
SUBVOLS = ["@slots", "@data", "@reserve", "@var", "@containers", "@vm",
           "@flatpak", "@nix", "@home", "@root", "@snapshots"]


def conf(key):
    with open(os.path.join(HERE, "builder", "asimaos.conf")) as f:
        value = re.search(rf'^{key}="([^"]*)"', f.read(), re.M).group(1)
    default = re.fullmatch(r"\$\{(\w+):-(.*)\}", value)     # "${OVERRIDE:-default}"
    return os.environ.get(default.group(1)) or default.group(2) if default else value


# Every wait is multiplied by ASIMAOS_TEST_SLOW (default 1): for slower hosts, for example a build
# machine that is itself a virtual machine (nested virtualization) with little memory.
SLOW = max(1.0, float(os.environ.get("ASIMAOS_TEST_SLOW", "1")))
# Every VM a test starts, so none is left running when a test stops early (a timeout).
LIVE_VMS = []

QGA = os.path.join(BUILD, "test-qga.sock")
MONITOR = os.path.join(BUILD, "test-monitor.sock")


SB_OWNER = "a7d15ee5-7a2c-4a52-9b8e-0a0a0a000001"   # owner of the added db entry (any GUID)


# The 4 MB Secure Boot firmware: it measures the boot into the TPM (the 2 MB build doesn't), so
# PCR 4 (shim, boot loader, boot image) and PCR 7 are real, as on a PC.
SB_CODE = "/usr/share/edk2/ovmf/OVMF_CODE_4M.secboot.qcow2"


def raw_vars(name):
    """A raw copy of one of OVMF's 4 MB variable stores, for virt-fw-vars to edit."""
    out = os.path.join(BUILD, f"test-{name}.raw")
    subprocess.run(["qemu-img", "convert", "-f", "qcow2", "-O", "raw", f"/usr/share/edk2/ovmf/{name}.qcow2", out], check=True)
    return out


def secureboot_vars():
    """OVMF's Secure Boot variables (Microsoft's keys enrolled) plus this build's certificate."""
    out = os.path.join(BUILD, "test-ovmf-vars-sb.fd")
    tool = os.path.join(BUILD, "tools", "virt-fw", "bin", "virt-fw-vars")
    subprocess.run([tool, "--input", raw_vars("OVMF_VARS_4M.secboot"), "--output", out,
                    "--add-db", SB_OWNER, os.path.join(BUILD, "keys", "secureboot.crt")],
                   check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    return out


def trust_vars():
    """A PC that trusts only Microsoft's 2023 UEFI certificate (no 2011 certificates, no Windows
    keys), with this build's certificate approved as a MOK, as the blue MokManager screen does."""
    out = os.path.join(BUILD, "test-ovmf-vars-trust.fd")
    tool = os.path.join(BUILD, "tools", "virt-fw", "bin", "virt-fw-vars")
    subprocess.run([tool, "--input", raw_vars("OVMF_VARS_4M"), "--output", out,
                    "--enroll-generate", "Asima Test Platform", "--microsoft-kek", "2023",
                    "--microsoft-db", "uefi23", "--add-mok", SB_OWNER, os.path.join(BUILD, "keys", "secureboot.crt"),
                    "--sb"], check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    return out


TPM_DIR = os.path.join(BUILD, "test-tpm")


class Vm:
    def __init__(self, disk, log_name, cdrom=None, passphrase=PASSPHRASE, share=None, keep_vars=False,
                 monitor=False, secureboot=False, vars_file=None, tpm=False, net=False, usb=None, watchdog=False,
                 pin=None):
        self.vars = os.path.join(BUILD, "test-ovmf-vars.qcow2")
        self.swtpm = None
        if vars_file:
            self.vars = vars_file
        elif secureboot:
            self.vars = secureboot_vars()
        elif not keep_vars:   # the same "firmware" across restarts of one installed system
            shutil.copy("/usr/share/edk2/ovmf/OVMF_VARS_4M.qcow2", self.vars)
        self.log = open(os.path.join(BUILD, log_name), "wb")
        self.buf = b""
        for sock in (QGA, MONITOR):
            if os.path.exists(sock):
                os.remove(sock)
        firmware = [
            "-machine", "q35,accel=kvm", "-cpu", "host",
            "-drive", "if=pflash,format=qcow2,unit=0,readonly=on,file=/usr/share/edk2/ovmf/OVMF_CODE_4M.qcow2",
            "-drive", f"if=pflash,format=qcow2,unit=1,file={self.vars}",
        ]
        if secureboot:   # the Secure Boot build of OVMF needs SMM
            firmware = [
                "-machine", "q35,accel=kvm,smm=on", "-cpu", "host",
                "-global", "driver=cfi.pflash01,property=secure,value=on",
                "-drive", f"if=pflash,format=qcow2,unit=0,readonly=on,file={SB_CODE}",
                "-drive", f"if=pflash,format=raw,unit=1,file={self.vars}",
            ]
        if tpm:   # a TPM 2.0 whose state outlives the VM (TPM_DIR), like a real chip
            os.makedirs(TPM_DIR, exist_ok=True)
            sock = os.path.join(TPM_DIR, "swtpm.sock")
            if os.path.exists(sock):
                os.remove(sock)
            self.swtpm = subprocess.Popen(["swtpm", "socket", "--tpm2", "--tpmstate", f"dir={TPM_DIR}",
                                           "--ctrl", f"type=unixio,path={sock}", "--terminate"],
                                          stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            for _ in range(50):
                if os.path.exists(sock):
                    break
                time.sleep(0.1)
            firmware += ["-chardev", f"socket,id=chrtpm,path={sock}", "-tpmdev", "emulator,id=tpm0,chardev=chrtpm",
                         "-device", "tpm-tis,tpmdev=tpm0"]
        # No passphrase (None): the disk must open without one (TPM), or the start waits for it.
        credential = ["-smbios", f"type=11,value=io.systemd.credential:cryptsetup.passphrase={passphrase}"] \
            if passphrase else []
        # A TPM PIN, typed at the start-up as the person would. systemd-cryptsetup asks for it as a
        # "LUKS2 token PIN" (credential cryptsetup.luks2-pin) when it unlocks through the token,
        # and as cryptsetup.tpm2-pin on its other path: both are given.
        if pin:
            credential += ["-smbios", f"type=11,value=io.systemd.credential:cryptsetup.luks2-pin={pin}",
                           "-smbios", f"type=11,value=io.systemd.credential:cryptsetup.tpm2-pin={pin}"]
        cmd = [
            "qemu-system-x86_64", *firmware,
            "-m", "4096", "-smp", "2",
            "-drive", f"file={disk},format=raw,if=virtio,discard=unmap",
            *credential,
            "-display", "none", "-serial", "stdio",
            *(["-nic", "user,model=virtio-net-pci"] if net else ["-nic", "none"]),
            "-monitor", f"unix:{MONITOR},server=on,wait=off" if monitor else "none",
            # The QEMU guest agent's channel (D22: guest tools start only in their own VM type).
            "-device", "virtio-serial-pci",
            "-chardev", f"socket,id=qga0,path={QGA},server=on,wait=off",
            "-device", "virtserialport,chardev=qga0,name=org.qemu.guest_agent.0",
        ]
        if share:   # a host folder, read-only, as the 9p share "updates"
            cmd += ["-virtfs", f"local,path={share},mount_tag=updates,security_model=none,readonly=on"]
        if cdrom:
            cmd += ["-drive", f"file={cdrom},media=cdrom,readonly=on"]
        if usb:   # a second disk with files on it, like a USB drive (/dev/vdb)
            cmd += ["-drive", f"file={usb},format=raw,if=virtio,readonly=on"]
        if watchdog:   # a hardware watchdog, like a real PC's (a freeze resets the PC)
            cmd += ["-device", "i6300esb", "-action", "watchdog=reset"]
        self.p = subprocess.Popen(cmd, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                  stderr=subprocess.STDOUT)
        LIVE_VMS.append(self)

    def expect(self, pattern, timeout):
        rx = re.compile(pattern.encode())
        end = time.time() + timeout * SLOW
        while time.time() < end:
            clean = ANSI.sub(b"", self.buf)
            m = rx.search(clean)
            if m:
                self.buf = clean[m.end():]
                return clean[:m.end()].decode(errors="replace")
            r, _, _ = select.select([self.p.stdout], [], [], 0.5)
            if r:
                data = os.read(self.p.stdout.fileno(), 65536)
                if not data:
                    break
                self.log.write(data)
                self.log.flush()
                self.buf += data
        tail = ANSI.sub(b"", self.buf)[-1500:].decode(errors="replace")
        raise TimeoutError(f"timed out waiting for {pattern!r}\n--- last console output ---\n{tail}")

    def send(self, text):
        self.p.stdin.write(text.encode())
        self.p.stdin.flush()

    def run(self, cmd, timeout=60):
        """Run a shell command on the serial console; return (output, exit code)."""
        tag = f"T{time.time_ns()}"
        self.send(f"echo {tag}B; {cmd}; rc=$?; printf '\\n{tag}E%d\\n' $rc\n")
        out = self.expect(rf"\n{tag}E\d+\n", timeout)
        body = out.split(f"{tag}B\n", 1)[-1]
        body, _, rc = body.rpartition(f"\n{tag}E")
        return body.strip("\n"), int(rc.strip())

    def banner(self, timeout=900):
        """The version on the console's login banner (/etc/issue), without signing in."""
        out = self.expect(r"Asima OS (\d{4}\.\d\d\.\d\d\.\d+) \(ttyS0\)", timeout)
        return re.findall(r"Asima OS (\d{4}\.\d\d\.\d\d\.\d+) \(ttyS0\)", out)[-1]

    def login(self, user="root", password=ROOT_PASSWORD, banner_seen=False):
        self.expect(r"login: ", 30 if banner_seen else 600)
        shell = r"\]# " if user == "root" else r"\]\$ "
        for attempt in range(3):
            self.send(user + "\n")
            self.expect(r"Password: ", 30)
            # login prints the prompt, then switches echo off and throws away what was typed
            # meanwhile: a password sent at once can be lost on a slow VM, and login then waits
            # 60 s for it and starts again. So a moment's pause, and another try if that happens.
            time.sleep(1)
            self.send(password + "\n")
            # The prompt again ("asimaos login: "), never the "Last login: …" line of a success.
            if re.search(shell, self.expect(shell + r"|(?m:^(?!Last )\S+ login: )", 120)):
                break
            if attempt == 2:
                raise TimeoutError(f"{user} could not log in on the serial console")
        if user != "root":
            self.send("sudo -i\n")
            self.expect(r"password for " + user, 30)
            time.sleep(1)
            self.send(password + "\n")
            self.expect(r"\]# ", 60)
        self.send("stty -echo; export PS1='# ' TERM=dumb SYSTEMD_PAGER= SYSTEMD_COLORS=0\n")
        time.sleep(1)

    def shutdown(self):
        self.send("systemctl poweroff\n")
        try:
            self.p.wait(timeout=120 * SLOW)
        except subprocess.TimeoutExpired:
            self.p.kill()

    def kill(self):
        # Wait until QEMU (and swtpm) are really gone: until then they keep their lock on the
        # firmware variables, and the next VM on the same "PC" could not start.
        for proc in (self.p, self.swtpm):
            if proc and proc.poll() is None:
                proc.kill()
            if proc:
                try:
                    proc.wait(timeout=60)
                except subprocess.TimeoutExpired:
                    pass

    def guest_agent(self, command):
        """One command to the QEMU guest agent inside the VM; returns the parsed answer."""
        import json
        import socket
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as s:
            s.settimeout(30)
            s.connect(QGA)
            s.sendall(json.dumps({"execute": command}).encode() + b"\n")
            data = b""
            while not data.endswith(b"\n"):
                chunk = s.recv(4096)
                if not chunk:
                    break
                data += chunk
        return json.loads(data)

    def monitor(self, command):
        import socket
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as s:
            s.settimeout(10)
            s.connect(MONITOR)
            s.sendall(command.encode() + b"\n")
            time.sleep(0.5)


results = []


def check(name, ok, detail=""):
    results.append((name, ok))
    mark = "\033[32mPASS\033[0m" if ok else "\033[31mFAIL\033[0m"
    print(f"  [{mark}] {name}")
    if detail and not ok:
        print("         " + detail.replace("\n", "\n         "))


def mount_of(vm, target):
    out, _ = vm.run(f"findmnt -n -o SOURCE,FSTYPE,OPTIONS --target {target} | tail -n1")
    return out


def check_common(vm, version):
    out, _ = vm.run("grep -E '^IMAGE_VERSION=' /etc/os-release")
    check(f"os-release reports IMAGE_VERSION={version}", f"IMAGE_VERSION={version}" in out, out)
    m = mount_of(vm, "/")
    check("/ is tmpfs (ephemeral root)", m.split()[1:2] == ["tmpfs"], m)
    m = mount_of(vm, "/usr")
    check("/usr is EROFS on /dev/mapper/usr, read-only",
          m.startswith("/dev/mapper/usr ") and " erofs " in m and " ro," in m, m)
    m = mount_of(vm, "/etc")
    check("/etc is OverlayFS over factory defaults", " overlay " in m, m)
    out, _ = vm.run("veritysetup status usr | grep -E 'type|status'")
    check("dm-verity active and verified", "verified" in out, out)
    _, rc = vm.run("touch /usr/asimaos-write-test 2>/dev/null")
    check("/usr rejects writes", rc != 0)
    _, rc = vm.run("test ! -e /usr/bin/pacman")
    check("pacman stripped from image", rc == 0)
    out, _ = vm.run("systemctl get-default; systemctl is-active plasmalogin.service", 120)
    check("graphical start with plasma-login-manager", out.split() == ["graphical.target", "active"], out)
    out, _ = vm.run("cat /sys/kernel/security/lsm")
    check("AppArmor among the security modules", "apparmor" in out.split(","), out)
    # Each Asima OS app has its own service, started by D-Bus activation on the first call.
    for name, app, expect in (("Updater1", "Asima Updater", f'\\"version\\":\\"{version}\\"'),
                              ("Maintenance1", "Asima Maintenance", f'\\"version\\":\\"{version}\\"'),
                              ("Manager1", "Asima Manager", '\\"shield\\"'),
                              ("Gaming1", "Asima Gaming", '\\"managed\\":true')):
        out, _ = vm.run(f"busctl --json=short call io.github.asimaos.{name} /io/github/asimaos/{name} io.github.asimaos.{name} Status", 60)
        check(f"{app}'s service answers (D-Bus activation)", expect in out, out[:300])


# The ESP never opens by itself; the test opens it by hand for a look, then closes it.
def esp(cmd):
    return f"mount /boot 2>/dev/null; ( {cmd} ); rc=$?; cd /; umount /boot 2>/dev/null; (exit $rc)"


def entries(vm):
    """{role: (file name, version)} of the start menu."""
    out, _ = vm.run(esp("cd /boot/loader/entries && grep -H '^version' asimaos-*.conf"))
    menu = {}
    for line in out.splitlines():
        name, _, rest = line.partition(":version")
        role = re.sub(r"(\+\d+(-\d+)?)?\.conf$", "", name).removeprefix("asimaos-")
        menu[role] = (name, rest.strip())
    return menu


def check_app_systems(vm):
    """D28: Snap, Nix and Waydroid are ready; only what must run natively is in the system."""
    out, _ = vm.run("systemctl is-active snapd.socket nix-daemon.socket; readlink /snap; "
                    "findmnt -n -o OPTIONS /nix | tr , '\\n' | grep -c '^subvol=/@nix$'; "
                    "getcap /usr/lib/snapd/snap-confine | grep -c cap_sys_admin; command -v waydroid nano")
    check("Snap and Nix start when used; /snap; /nix on its own subvolume; snap-confine's capabilities",
          out.split()[:6] == ["active", "active", "/var/lib/snapd/snap", "1", "1", "/usr/bin/waydroid"], out)
    out, _ = vm.run("snap version 2>&1 | head -2; nix --version")
    check("snap and nix answer", "snapd" in out and "nix (Nix)" in out, out)
    out, _ = vm.run("mkdir -p /run/binder-test && mount -t binder binder /run/binder-test 2>&1 "
                    "&& ls /run/binder-test && umount /run/binder-test")
    check("the kernel's Android binder works (binderfs, for Waydroid)", "binder-control" in out, out)
    out, _ = vm.run("find /usr -xdev -type f -perm -4000 | wc -l; "
                    "grep -L '^NoDisplay=true' /usr/share/applications/{mpv,cups,lstopo,qv4l2}.desktop 2>/dev/null | wc -l; "
                    "findmnt -n -o FSTYPE /var/cache; findmnt -n -o FSTYPE /var/cache/apparmor", 600)
    check("13 programs with root rights; every app in the menu, as in stock KDE; caches in RAM, AppArmor's on disk",
          out.split() == ["13", "4", "tmpfs", "btrfs"], out)
    out, _ = vm.run("mkdir -p /var/tmp/asimaos-restore-test-1/a /var/tmp/keep-test; "
                    "systemd-tmpfiles --boot --remove /usr/lib/tmpfiles.d/asimaos-tmp.conf; "
                    "ls /var/tmp | grep -c '^asimaos-'; ls /var/tmp | grep -c '^keep-test$'; rmdir /var/tmp/keep-test")
    check("what an interrupted job left in /var/tmp is removed at start-up, nothing else",
          out.split()[:2] == ["0", "1"], out)
    out, _ = vm.run("for i in $(seq 30); do grep -q lavd /sys/kernel/sched_ext/root/ops 2>/dev/null && break; sleep 1; done; "
                    "cat /sys/kernel/sched_ext/state /sys/kernel/sched_ext/root/ops 2>&1", 60)
    check("the gaming CPU scheduler runs from the start, for everyone (scx_lavd, D44)",
          out.split()[:1] == ["enabled"] and "lavd" in out, out)
    # The disk guard, without a person at the keyboard (stdin is not a terminal).
    out, _ = vm.run("rm -r /nix </dev/null; echo rc=$?; findmnt -n -o FSTYPE /nix; "
                    "wipefs --no-act \"$(blkid -t PARTLABEL=ESP -o device | head -n1)\" </dev/null; echo rc=$?")
    check("disk guard: rm -r /nix and wipefs of the Asima OS disk are refused",
          out.count("refused") == 2 and "rc=1" in out and "btrfs" in out, out)


def check_everything_on(vm):
    """D26: no profiles, everything is on for everyone."""
    out, _ = vm.run("stat -c %a /dev/ntsync; udevadm info -q property /dev/uinput | grep -c '^CURRENT_TAGS=.*:uaccess:'; "
                    "systemctl is-active virtqemud.socket; systemctl --global is-enabled podman.socket; "
                    "command -v gcc >/dev/null || echo no-compilers; "
                    "cat /proc/sys/kernel/split_lock_mitigate 2>/dev/null || echo none")
    words = out.split()
    check("gaming on: /dev/ntsync for everyone, /dev/uinput for the person at the computer",
          words[:2] == ["666", "1"], out)
    check("development on: libvirt sockets, Podman socket for every user; compilers come from Nix or Distrobox (D28)",
          words[2:5] == ["active", "enabled", "no-compilers"], out)
    check("split-lock slow-down off (or not on this CPU)", words[5:6] in (["0"], ["none"]), out)
    out, _ = vm.run("swapon --show=NAME --noheadings; systemctl is-active systemd-oomd.service; "
                    "systemctl is-enabled asimaos-snapshot.timer asimaos-backup.timer asimaos-scrub.timer "
                    "asimaos-balance.timer asimaos-ssd-audit.timer asimaos-boot-timeout.timer | sort -u")
    check("zram swap, systemd-oomd, maintenance timers on",
          out.split() == ["/dev/zram0", "active", "enabled"], out)


def check_vm_tools(vm):
    """D22: the QEMU guest agent answers; the other VM types' tools stay off."""
    try:
        answer = vm.guest_agent("guest-ping")
    except OSError as e:
        answer = {"error": str(e)}
    check("QEMU guest agent answers (guest-ping)", answer == {"return": {}}, str(answer))
    out, _ = vm.run("for u in vboxservice vmtoolsd vmware-vmblock-fuse hv_kvp_daemon; do "
                    "systemctl show -P ConditionResult $u.service; done | sort -u; systemd-detect-virt")
    check("VirtualBox, VMware and Hyper-V tools stay off in QEMU", out.split() == ["no", "kvm"], out)


def check_installed(vm, version, reboot=True, encrypted=True, user=None, password=None):
    check_common(vm, version)
    m = mount_of(vm, "/var")
    check("/var is Btrfs subvolume @var", "subvol=/@var" in m, m)
    m = mount_of(vm, "/home")
    check("/home is Btrfs subvolume @home", "subvol=/@home" in m, m)
    out, _ = vm.run("ls -A /boot | wc -l; findmnt -n /boot >/dev/null && echo open || echo closed; "
                    "grep -c ' /boot ' /proc/$(pidof -s asimaos-updaterd || echo 1)/mountinfo 2>/dev/null || true")
    check("/boot stays closed for the desktop", out.split()[:2] == ["0", "closed"], out)
    if encrypted:
        out, _ = vm.run("cryptsetup status crypt_pool | grep -E 'type|flags'")
        check("pool is LUKS2 with discards", "LUKS2" in out and "discards" in out, out)
    else:
        out, _ = vm.run("test ! -e /dev/mapper/crypt_pool && lsblk -nro PARTLABEL,FSTYPE | grep '^asimaos_pool '")
        check("pool is plain Btrfs (installed without encryption, D9)", "asimaos_pool btrfs" in out, out)
    if user:
        out, _ = vm.run(f"id -nG {user}; stat -c '%U %a' /home/{user}")
        check(f"account {user}: administrator (wheel), private home",
              "wheel" in out.split() and f"{user} 700" in out, out)
    out, _ = vm.run("btrfs subvolume list /home | awk '{print $NF}' | sort | tr '\\n' ' '")
    check(f"all {len(SUBVOLS)} subvolumes exist", all(s in out.split() for s in SUBVOLS), out)
    out, _ = vm.run("systemctl is-system-running --wait; systemctl --failed --no-legend --plain", 300)
    check("system running, no failed units", out.splitlines()[0] == "running", out)
    out, _ = vm.run("journalctl -b -o cat --no-pager | grep -i 'ordering cycle' | head -5")
    check("no ordering loops between units", out.strip() == "", out)
    check_everything_on(vm)
    check_app_systems(vm)
    check_vm_tools(vm)
    out, _ = vm.run("for i in $(seq 120); do systemctl -q is-active boot-complete.target && break; sleep 2; done; "
                    "cat /run/asimaos/health/core.json /run/asimaos/health/desktop.json", 300)
    check("health checks passed (system files, login screen up for 15 s)", out.count('"ok": true') == 2, out)
    menu = entries(vm)
    check(f"start menu: 'Asima OS {version} (current)' marked good (+3-0 counter removed)",
          menu.get("current") == ("asimaos-current.conf", version), str(menu))
    out, _ = vm.run(esp("grep -h '^title' /boot/loader/entries/*.conf; bootctl status 2>/dev/null | grep -E '^ +Current Entry:'"))
    check("start menu shows the version number, no alpha/beta",
          f"Asima OS {version} (current)" in out and not re.search("alpha|beta", out, re.I), out)
    out, _ = vm.run("asimaos; asimaos versions; asimaos health | head -n 3", 180)
    check("the asimaos command: status, versions and the health check",
          f"Asima OS {version}" in out and f"{version}  current" in out and "system-files" in out, out)
    out, _ = vm.run("findmnt -n -o OPTIONS /home/.snapshots | tr , '\\n' | grep -x ro; "
                    "stat -c %s /run/initramfs/reserve/.emergency_ballast")
    check("snapshots at /home/.snapshots (read-only); 2 GB emergency reserve", out.split() == ["ro", "2147483648"], out)
    if not reboot:
        return
    vm.run("echo asimaos-persist-ok > /etc/asimaos-persist-test; echo hi > /home/persist-home; sync")
    print("\n  -- reboot: verify persistence --")
    vm.send("systemctl reboot\n")
    vm.login(user or "root", password or ROOT_PASSWORD)
    out, _ = vm.run("cat /etc/asimaos-persist-test /home/persist-home 2>&1")
    check("/etc change persisted across reboot (@data overlay)", "asimaos-persist-ok" in out, out)
    check("/home persisted across reboot", "hi" in out.split(), out)
    m = mount_of(vm, "/")
    check("root is a fresh tmpfs again", m.split()[1:2] == ["tmpfs"], m)


def test_disk(version, secureboot=False):
    src = os.path.join(BUILD, "out", "asimaos-disk.raw")
    disk = os.path.join(BUILD, "test-disk.raw")
    subprocess.run(["cp", "--reflink=auto", src, disk], check=True)
    print(f"Asima OS disk-image test: {version}" + (" (Secure Boot enforced)" if secureboot else ""))
    vm = Vm(disk, "test-console-disk-sb.log" if secureboot else "test-console-disk.log", monitor=True,
            secureboot=secureboot)
    try:
        print("\nBoot installed image")
        vm.login()
        if secureboot:
            out, _ = vm.run("mokutil --sb-state; bootctl status 2>/dev/null | grep -E 'Secure Boot:|Stub:|Product:'")
            check("Secure Boot enforced: shim → systemd-boot → the signed UKI started", "SecureBoot enabled" in out, out)
            check_common(vm, version)
            check_rescue(vm, version)
        else:
            check_installed(vm, version)
            check_rescue(vm, version)
        vm.shutdown()
    finally:
        vm.kill()


def check_rescue(vm, version):
    """Asima Rescue, started once from the start menu: the app on the screen, the engine sees
    the disk; then back to Asima OS."""
    print("\n  -- Asima Rescue (started once) --")
    out, _ = vm.run(esp("grep -h '^title' /boot/loader/entries/asimaos-rescue.conf; bootctl set-oneshot asimaos-rescue.conf; echo $?"))
    check("start menu has 'Asima Rescue'", "Asima Rescue" in out and out.split()[-1] == "0", out)
    vm.send("systemctl reboot\n")
    vm.expect(rf"Asima Rescue {re.escape(version)} \(ttyS0\)", 600)
    vm.expect(r"\]# ", 60)                     # root on the serial console (autologin)
    vm.send("stty -echo; export PS1='# ' TERM=dumb SYSTEMD_PAGER= SYSTEMD_COLORS=0\n")
    time.sleep(1)
    out, _ = vm.run("for i in $(seq 60); do systemctl -q is-active asimaos-rescue.service && break; sleep 1; done; "
                    "sleep 5; systemctl is-active asimaos-rescue.service; findmnt -n -o FSTYPE /", 120)
    check("Asima Rescue runs in RAM with its app on the screen", out.split() == ["active", "rootfs"]
          or out.split() == ["active", "tmpfs"], out)
    vm.monitor(f'screendump "{BUILD}/test-rescue.png" -f png')
    out, rc = vm.run("/usr/lib/asimaos/asimaos-rescue-engine status --json", 120)
    check("the Rescue engine finds this disk and its version", rc == 0 and f'"version":"{version}"' in out, out[:400])
    vm.send("systemctl reboot\n")
    vm.login()
    out, _ = vm.run("grep -o 'asimaos.version=[^ ]*' /proc/cmdline")
    check("after Rescue, Asima OS starts again", f"asimaos.version={version}" in out, out)


def unseal_fails(vm):
    """Try to open the pool with the TPM alone; True when the TPM refuses."""
    out, _ = vm.run("dev=$(blkid -t PARTLABEL=crypt_pool -o device | head -n1); "
                    "/usr/lib/systemd/systemd-cryptsetup attach asimaos-tpm-probe \"$dev\" none "
                    "tpm2-device=auto,headless=true,tries=1 >/dev/null 2>&1; echo rc=$?; "
                    "test -e /dev/mapper/asimaos-tpm-probe && { echo OPENED; "
                    "/usr/lib/systemd/systemd-cryptsetup detach asimaos-tpm-probe; }", 120)
    return "OPENED" not in out and "rc=0" not in out, out


def test_trust(version):
    """On a PC that trusts only Microsoft's 2023 certificate."""
    src = os.path.join(BUILD, "out", "asimaos-disk.raw")
    disk = os.path.join(BUILD, "test-disk.raw")
    subprocess.run(["cp", "--reflink=auto", src, disk], check=True)
    shutil.rmtree(TPM_DIR, ignore_errors=True)
    vars_file = trust_vars()
    print(f"Asima OS trust test: {version} (Microsoft UEFI CA 2023 only, Asima OS key as MOK, TPM 2.0)")
    vm = Vm(disk, "test-console-trust.log", monitor=True, secureboot=True, vars_file=vars_file, tpm=True)
    try:
        print("\nBoot 1: with the passphrase")
        vm.login()
        out, _ = vm.run("mokutil --sb-state; mokutil --db | grep -c 'Microsoft UEFI CA 2023'; "
                        "mokutil --db | grep -c 'Microsoft Corporation UEFI CA 2011'")
        check("the dual-signed shim starts on a PC that trusts only the 2023 certificate (19)",
              "SecureBoot enabled" in out and out.split()[-2:] == ["1", "0"], out)
        out, _ = vm.run("tpm2_pcrread sha256:7,11 | grep -c ': 0x0000000000000000000000000000000000000000000000000000000000000000'")
        check("the boot is measured into the TPM (PCR 7 and 11 are not empty)", out.strip() == "0", out)
        out, _ = vm.run("cat /sys/kernel/security/lockdown; journalctl -b -o cat | grep -m1 'kernel lockdown'")
        check("lockdown (integrity) on: Secure Boot and the Asima OS key approved (18)", "[integrity]" in out, out)
        out, _ = vm.run("journalctl -k -b -o cat | grep -m2 -i 'Asima OS Secure Boot'")
        check("the kernel loaded the approved Asima OS key from the MOK list", "Asima OS Secure Boot" in out, out)
        out, _ = vm.run("modprobe nvidia 2>&1; journalctl -k -b -o cat | grep -i -E "
                        "'nvidia|module verification|key was rejected|lockdown: ' | tail -n 6", 120)
        check("the signed NVIDIA driver passes the kernel's signature check under lockdown",
              not re.search("Key was rejected|Lockdown: .*unsigned|required key not available|verification failed", out, re.I)
              and re.search("NVRM|No such device", out, re.I) is not None, out)

        print("\n  -- keyboard layout for the start-up passphrase (21 b) --")
        out, _ = vm.run("localectl set-keymap de; sleep 3; cat /etc/vconsole.conf; "
                        "tail -c +5 /sys/firmware/efi/efivars/AsimaKeyboard-40956354-ea4c-4e70-bb8d-f7d55625a8bf; echo")
        check("a new system keyboard layout is stored for the start-up passphrase",
              "XKBLAYOUT=de" in out and re.search(r"^\S+ de \S+$", out, re.M) is not None, out)

        print("\n  -- TPM unlock (5, 6) --")
        out, rc = vm.run(f"busctl --timeout=600 call io.github.asimaos.Maintenance1 /io/github/asimaos/Maintenance1 io.github.asimaos.Maintenance1 "
                         f"EnrollTpm s {PASSPHRASE}; echo rc=$?", 660)
        check("Asima Maintenance switches TPM unlock on", "rc=0" in out, out)
        out, _ = vm.run("dev=$(blkid -t PARTLABEL=crypt_pool -o device | head -n1); "
                        "cryptsetup luksDump --dump-json-metadata \"$dev\" | tr -d ' \\n' | grep -o '\"type\":\"systemd-tpm2\"[^}]*'")
        check("the TPM key slot is bound to the systemd-pcrlock policy alone (no signed policy)",
              '"tpm2_pcrlock":true' in out and '"tpm2_pubkey' not in out, out)
        out, _ = vm.run(esp("ls /var/lib/systemd/pcrlock.json /boot/loader/credentials/ 2>&1; "
                            "ls /var/lib/pcrlock.d/650-asimaos-uki.pcrlock.d | wc -l"))
        check("the policy (PCR 4 + 7) is in place, with its copy for the start-up on the ESP",
              "/var/lib/systemd/pcrlock.json" in out and "pcrlock." in out and "No such file" not in out, out)
        vm.shutdown()
    finally:
        vm.kill()
    if not all(ok for _, ok in results):
        return

    print("\nBoot 2: no passphrase anywhere: the TPM must open the disk")
    vm = Vm(disk, "test-console-trust2.log", monitor=True, secureboot=True, vars_file=vars_file, tpm=True,
            passphrase=None)
    try:
        vm.login()
        out, _ = vm.run("journalctl -b -o cat | grep -m1 'asimaos-keyboard:'")
        check("the start-up passphrase prompt used the system's layout (de)", "layout de" in out, out)
        out, _ = vm.run("journalctl -b -o cat | grep -i -m3 -E 'tpm2|unseal'")
        check("started without a passphrase: the TPM opened the disk", re.search("tpm2", out, re.I) is not None, out)
        out, _ = vm.run("journalctl -b -o cat | grep -m1 'start-up policy'")
        check("the start-up read the pcrlock policy copy from the ESP", "is ready" in out, out)
        print("\n  -- TPM unlock set up again: two steps --")
        tpm_slots = ("dev=$(blkid -t PARTLABEL=crypt_pool -o device | head -n1); "
                     "cryptsetup luksDump --dump-json-metadata \"$dev\" | grep -o 'systemd-tpm2' | wc -l")
        enroll = (f"busctl --timeout=600 call io.github.asimaos.Maintenance1 /io/github/asimaos/Maintenance1 "
                  f"io.github.asimaos.Maintenance1 EnrollTpm s {PASSPHRASE}; echo rc=$?; {tpm_slots}; "
                  "test -f /var/lib/asimaos/tpm2-rotation.json && echo PENDING")
        out, _ = vm.run(enroll, 660)
        check("set up again while it works: nothing changes (the same slot, nothing pending)",
              "rc=0" in out and out.split()[-1:] == ["1"], out)
        # A slot with another policy (PCR 7 alone), as after the policy changed: setting up again
        # replaces it in two steps.
        vm.run(f"dev=$(blkid -t PARTLABEL=crypt_pool -o device | head -n1); PASSWORD={PASSPHRASE} "
               "systemd-cryptenroll --wipe-slot=tpm2 --tpm2-device=auto --tpm2-pcrs=7 \"$dev\"", 600)
        out, _ = vm.run(enroll, 660)
        check("from a slot with another policy: the new one is added and tried, the old one stays until the next start",
              "rc=0" in out and out.split()[-2:] == ["2", "PENDING"], out)
        vm.send("systemctl reboot\n")
        vm.banner(900)
        vm.login(banner_seen=True)
        out, _ = vm.run(f"{tpm_slots}; test -f /var/lib/asimaos/tpm2-rotation.json && echo PENDING; "
                        "journalctl -b -o cat -u asimaos-tpm-rotation.service | grep -c 'works after a restart'")
        check("after a start without a passphrase, the old TPM slot is gone", out.split() == ["1", "1"], out)
        print("\n  -- Asima Rescue must not get the disk key --")
        vm.run("bootctl set-oneshot asimaos-rescue.conf")
        vm.send("systemctl reboot\n")
        vm.expect(rf"Asima Rescue {re.escape(version)} \(ttyS0\)", 600)
        vm.expect(r"\]# ", 60)
        vm.send("stty -echo; export PS1='# ' TERM=dumb SYSTEMD_PAGER= SYSTEMD_COLORS=0\n")
        time.sleep(1)
        ok, out = unseal_fails(vm)
        check("Asima Rescue can't unseal the disk key (its boot image isn't in the TPM policy)", ok, out)
        vm.send("systemctl poweroff\n")
        vm.p.wait(timeout=120)
    finally:
        vm.kill()
    if not all(ok for _, ok in results):
        return

    # D49: the optional way, PCR 4 + 7 and a PIN typed at every start.
    pin = "2468"   # TEST-ONLY
    print("\nBoot 3: TPM unlock with a PIN instead (Asima Maintenance)")
    vm = Vm(disk, "test-console-trust3.log", monitor=True, secureboot=True, vars_file=vars_file, tpm=True)
    try:
        vm.login()
        out, _ = vm.run(f"busctl --timeout=900 call io.github.asimaos.Maintenance1 /io/github/asimaos/Maintenance1 "
                        f"io.github.asimaos.Maintenance1 EnrollTpmPin ss {PASSPHRASE} {pin}; echo rc=$?; "
                        "dev=$(blkid -t PARTLABEL=crypt_pool -o device | head -n1); "
                        "cryptsetup luksDump --dump-json-metadata \"$dev\" | tr -d ' \\n' > /tmp/luks.json; "
                        "grep -o '\"type\":\"systemd-tpm2\"' /tmp/luks.json | wc -l; grep -c '\"tpm2-pin\":true' /tmp/luks.json; "
                        "test -f /var/lib/asimaos/tpm2-rotation.json && echo PENDING", 960)
        check("Asima Maintenance switches to a PIN: one TPM slot, which asks for the PIN, nothing pending",
              "rc=0" in out and out.split()[-2:] == ["1", "1"], out)
        vm.shutdown()
    finally:
        vm.kill()
    if not all(ok for _, ok in results):
        return
    print("\nBoot 4: only the PIN at the start (no passphrase anywhere)")
    vm = Vm(disk, "test-console-trust4.log", monitor=True, secureboot=True, vars_file=vars_file, tpm=True,
            passphrase=None, pin=pin)
    try:
        # Without a passphrase the start would wait at its prompt: reaching the login proves it.
        vm.login()
        out, _ = vm.run("journalctl -b -o cat | grep -i -m3 -E 'tpm2|unseal'")
        check("started with the PIN alone: the TPM opened the disk", re.search("tpm2", out, re.I) is not None, out)
        vm.shutdown()
    finally:
        vm.kill()
    if not all(ok for _, ok in results):
        return
    # Last: wrong PINs lock the (emulated) TPM for a while, as they would a real one.
    print("\nBoot 5: a wrong PIN, and the passphrase")
    vm = Vm(disk, "test-console-trust5.log", monitor=True, secureboot=True, vars_file=vars_file, tpm=True,
            passphrase=PASSPHRASE, pin="9999")
    try:
        vm.login()
        out, _ = vm.run("tpm2_getcap properties-variable 2>/dev/null | grep -E 'inLockout|LOCKOUT_COUNTER'")
        check("after a wrong PIN the passphrase still opens the disk (the TPM counts the wrong PIN)",
              "LOCKOUT_COUNTER" in out and "LOCKOUT_COUNTER: 0x0" not in out, out)
        vm.shutdown()
    finally:
        vm.kill()


def test_own_keys(version):
    """Own Secure Boot keys: prepared with Secure Boot on, written in Setup Mode, then a start with
    Secure Boot on through this computer's own Platform Key."""
    src = os.path.join(BUILD, "out", "asimaos-disk.raw")
    disk = os.path.join(BUILD, "test-disk.raw")
    subprocess.run(["cp", "--reflink=auto", src, disk], check=True)
    vars_file = trust_vars()
    tool = os.path.join(BUILD, "tools", "virt-fw", "bin", "virt-fw-vars")
    maint = ("busctl --timeout=300 call io.github.asimaos.Maintenance1 /io/github/asimaos/Maintenance1 "
             "io.github.asimaos.Maintenance1")
    print(f"Asima OS own Secure Boot keys test: {version}")
    vm = Vm(disk, "test-console-ownkeys.log", secureboot=True, vars_file=vars_file)
    try:
        print("\nBoot 1: prepare (Secure Boot on)")
        vm.login()
        out, _ = vm.run(f"{maint} PrepareOwnKeys; echo rc=$?; ls /var/lib/asimaos/secureboot/; "
                        "stat -c %a /var/lib/asimaos/secureboot/PK.key", 120)
        check("the firmware's lists are saved and this computer's keys made, root-only",
              "rc=0" in out and "saved-db.esl" in out and "KEK.crt" in out and out.split()[-1] == "600", out)
        vm.shutdown()
    finally:
        vm.kill()
    if not all(ok for _, ok in results):
        return
    # What the person does in the firmware settings: "Reset to Setup Mode" (the Platform Key goes).
    subprocess.run([tool, "--input", vars_file, "--output", vars_file, "-d", "PK"],
                   check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    vm = Vm(disk, "test-console-ownkeys2.log", secureboot=True, vars_file=vars_file)
    try:
        print("\nBoot 2: Setup Mode, write the keys")
        vm.login()
        out, _ = vm.run(f"{maint} Status | grep -o 'setupMode[^,]*' | head -1; {maint} EnrollOwnKeys as 0; echo rc=$?", 300)
        check("in Setup Mode the keys are written", "true" in out.split("rc=")[0] and "rc=0" in out, out)
        vm.shutdown()
    finally:
        vm.kill()
    # What the firmware now holds, read from its variable store (not through the guest's tools).
    store = subprocess.run([tool, "--input", vars_file, "--print", "-v"], capture_output=True, text=True).stdout
    lists = {}
    for line in store.splitlines():
        m = re.match(r"name=(\w+) ", line)
        if m:
            name = m.group(1)
        elif line.strip().startswith("subject "):
            lists.setdefault(name, []).append(line.strip()[len("subject "):])
    check("the Platform Key is this computer's; db has Microsoft's and the Asima OS certificate",
          len(lists.get("PK", [])) == 1 and "Platform Key (Asima OS)" in lists["PK"][0]
          and any("Microsoft UEFI CA 2023" in s for s in lists.get("db", []))
          and any(s.startswith("CN=Asima OS Secure Boot") for s in lists.get("db", [])), str(lists))
    if not all(ok for _, ok in results):
        return
    vm = Vm(disk, "test-console-ownkeys3.log", secureboot=True, vars_file=vars_file)
    try:
        print("\nBoot 3: Secure Boot with the own keys")
        vm.login()
        out, _ = vm.run("mokutil --sb-state; cat /sys/kernel/security/lockdown; "
                        f"{maint} Status | grep -o 'own[^,]*' | head -1")
        check("it starts with Secure Boot on, locked down, and knows its own keys",
              "SecureBoot enabled" in out and "[integrity]" in out and "true" in out.split("own")[-1], out)
        vm.shutdown()
    finally:
        vm.kill()


def test_reset(version):
    """Reset settings puts /etc and a person's settings back to the defaults and keeps the rest; then
    the factory reset keeps nothing and leaves a fresh install of the same version."""
    src = os.path.join(BUILD, "out", "asimaos-disk.raw")
    disk = os.path.join(BUILD, "test-disk.raw")
    subprocess.run(["cp", "--reflink=auto", src, disk], check=True)
    print(f"Asima OS factory reset test: {version}")
    fresh_pass = "fresh-disk-pass"
    vm = Vm(disk, "test-console-reset.log", keep_vars=False)
    try:
        print("\nBoot 1: someone's settings, then Reset settings")
        vm.login()
        # olduser may ask without a password here (the rule is itself a change in /etc, so the
        # reset removes it too).
        vm.run("useradd -m olduser && echo secret > /home/olduser/notes.txt && "
               "runuser -u olduser -- sh -c 'mkdir -p ~/.config ~/.local/state && echo old > ~/.config/kdeglobals && "
               "echo old > ~/.config/kxkbrc && echo old > ~/.local/state/dolphinstaterc' && "
               "mkdir -p /etc/asimaos && echo TEST=1 > /etc/asimaos/test-setting.conf && "
               "printf '%s\\n' 'polkit.addRule(function(a, s) { if (a.id == \"io.github.asimaos.maintenance.etc\" "
               "&& s.user == \"olduser\") return polkit.Result.YES; });' > /etc/polkit-1/rules.d/00-asimaos-test.rules; "
               "sleep 2")
        before, _ = vm.run("cat /etc/hostname")
        vm.run("(runuser -u olduser -- busctl call io.github.asimaos.Maintenance1 /io/github/asimaos/Maintenance1 "
               "io.github.asimaos.Maintenance1 ResetSettings >/tmp/reset-settings.out 2>&1 &)")
        vm.banner(900)
        vm.login(banner_seen=True)
        out, _ = vm.run("journalctl -b -o cat -u asimaos-early-mount.service | grep -c 'reset-settings-etc'; "
                        "ls /etc/asimaos/test-setting.conf /etc/polkit-1/rules.d/00-asimaos-test.rules 2>/dev/null | wc -l; "
                        "ls /run/initramfs/data/flags/ | grep -c reset-settings; cat /etc/hostname")
        check("Reset settings: /etc's changes went back to the defaults at the start, the computer's name stayed",
              out.split()[:3] == ["1", "0", "0"] and out.split()[3:] == before.split(), f"{out} (before: {before})")
        out, _ = vm.run("cd /home/olduser; ls .config/kdeglobals .local/state/dolphinstaterc 2>/dev/null | wc -l; "
                        "cat .config/kxkbrc notes.txt; stat -c %U .local/share/asimaos/old-settings/*/.config/kdeglobals; "
                        "journalctl -b -o cat -u asimaos-reset-settings.service | grep -c 'olduser: settings reset'")
        check("…and olduser's desktop settings, kept aside in a folder; the keyboard and the files stayed",
              out.split() == ["0", "old", "secret", "olduser", "1"], out)
        print("\nBoot 2: the factory reset")
        # The old system's own name, keyboard (also the start-up one) and time zone: all must go.
        vm.run("hostnamectl set-hostname oldpc && localectl set-keymap de && timedatectl set-timezone Europe/Berlin; sleep 3")
        vm.run("asimaos snapshot", 300)
        request = ('{"fullName":"Fresh Person","username":"fresh","password":"fresh-pw",'
                   f'"passphrase":"{fresh_pass}","current":"{PASSPHRASE}"}}')
        out, rc = vm.run(f"busctl --timeout=600 call io.github.asimaos.Maintenance1 /io/github/asimaos/Maintenance1 "
                         f"io.github.asimaos.Maintenance1 PrepareFactoryReset s '{request}'; echo rc=$?", 660)
        check("a fresh account and new disk secrets are staged (a new recovery key comes back)",
              "rc=0" in out and "recoveryKey" in out, out[-400:])
        vm.run("(busctl call io.github.asimaos.Maintenance1 /io/github/asimaos/Maintenance1 io.github.asimaos.Maintenance1 "
               "ScheduleFactoryReset >/dev/null 2>&1 &)")
        # The reset runs in the initramfs at the restart (its messages go to the journal, not the
        # console); the fresh account signs in afterwards.
        vm.banner(900)
        vm.login("fresh", "fresh-pw", banner_seen=True)
        out, _ = vm.run("journalctl -b -o cat -u asimaos-early-mount.service | grep -m1 'resetting to a fresh installation'")
        check("the reset ran at the start (initramfs)", "resetting to a fresh installation" in out, out)
        out, _ = vm.run("id -nG fresh; id olduser 2>&1 | head -1; ls /home; "
                        "ls /run/asimaos-snapshots/home 2>/dev/null | wc -l; grep -o 'asimaos.version=[^ ]*' /proc/cmdline")
        check("after the reset: only the fresh account (an administrator), no old files, no snapshots, same version",
              "wheel" in out and "no such user" in out and "olduser" not in out.split("\n", 2)[-1]
              and f"asimaos.version={version}" in out, out)
        out, _ = vm.run(f"dev=$(blkid -t PARTLABEL=crypt_pool -o device | head -n1); "
                        f"printf %s {PASSPHRASE} | cryptsetup open --test-passphrase --key-file=- \"$dev\" && echo OLD-WORKS; "
                        f"printf %s {fresh_pass} | cryptsetup open --test-passphrase --key-file=- \"$dev\" && echo NEW-WORKS",
                        300)   # two Argon2id tries of 1 GiB each: over a minute apiece in a VM on a small host
        check("the old disk passphrase no longer opens the disk; the new one does",
              "OLD-WORKS" not in out and "NEW-WORKS" in out, out)
        out, _ = vm.run("cat /etc/hostname; grep -c '^KEYMAP=us$' /etc/vconsole.conf; readlink /etc/localtime; "
                        "ls /sys/firmware/efi/efivars/ | grep -c '^AsimaKeyboard-'")
        check("nothing of the old system is kept: the shipped name, US keyboard (also at start-up) and UTC",
              out.split() == ["asimaos", "1", "../usr/share/zoneinfo/UTC", "0"], out)
        vm.shutdown()
    finally:
        vm.kill()


def test_rescue_reinstall(version):
    """Asima Rescue unlocks the disk and reinstalls the running version from a USB drive."""
    src = os.path.join(BUILD, "out", "asimaos-disk.raw")
    disk = os.path.join(BUILD, "test-disk.raw")
    update = os.path.join(BUILD, "out", f"asimaos-{version}-x86_64.aosupd")
    usb = os.path.join(BUILD, "test-usb.img")
    subprocess.run(["cp", "--reflink=auto", src, disk], check=True)
    folder = os.path.join(BUILD, "test-usb")
    shutil.rmtree(folder, ignore_errors=True)
    os.makedirs(folder)
    os.link(update, os.path.join(folder, os.path.basename(update)))
    if os.path.exists(usb):
        os.remove(usb)
    size = os.path.getsize(update) + 512 * 1024 * 1024
    subprocess.run(["mke2fs", "-q", "-F", "-t", "ext4", "-L", "UPDATES", "-d", folder, usb, f"{size // 1024}k"], check=True)
    print(f"Asima Rescue test: unlock and reinstall {version}")
    vm = Vm(disk, "test-console-rescue.log", usb=usb)
    try:
        vm.login()
        vm.run(esp("bootctl set-oneshot asimaos-rescue.conf"))
        vm.send("systemctl reboot\n")
        vm.expect(rf"Asima Rescue {re.escape(version)} \(ttyS0\)", 600)
        vm.expect(r"\]# ", 60)
        vm.send("stty -echo; export PS1='# ' TERM=dumb SYSTEMD_PAGER= SYSTEMD_COLORS=0\n")
        time.sleep(1)
        E = "/usr/lib/asimaos/asimaos-rescue-engine"
        out, rc = vm.run(f"printf '%s\\n' {PASSPHRASE} | {E} unlock --json --secret-stdin | tail -n 2", 300)
        check("Rescue unlocks the disk with the passphrase", rc == 0 and '"done"' in out, out)
        out, rc = vm.run(f"mkdir -p /run/usb && mount -o ro /dev/vdb /run/usb && "
                         f"{E} reinstall /run/usb/{os.path.basename(update)} --json | tail -n 3", 1800)
        check("Rescue reinstalls the running version from the USB drive", rc == 0 and '"done"' in out, out)
        vm.send("systemctl reboot\n")
        vm.login()
        out, _ = vm.run("grep -o 'asimaos.version=[^ ]*' /proc/cmdline; veritysetup status usr | grep -c verified")
        check("after the reinstall Asima OS starts, verified", f"asimaos.version={version}" in out and out.split()[-1] == "1", out)
        vm.shutdown()
    finally:
        vm.kill()


def test_flathub(version):
    """Flathub works out of the box: an app installs and starts."""
    src = os.path.join(BUILD, "out", "asimaos-disk.raw")
    disk = os.path.join(BUILD, "test-disk.raw")
    subprocess.run(["cp", "--reflink=auto", src, disk], check=True)
    print(f"Flathub test: {version}")
    vm = Vm(disk, "test-console-flathub.log", net=True)
    try:
        vm.login()
        out, _ = vm.run("flatpak remotes --columns=name,url")
        check("Flathub is set up out of the box", "flathub" in out and "dl.flathub.org" in out, out)
        out, rc = vm.run("flatpak install -y --noninteractive flathub org.kde.kcalc 2>&1 | tail -n 2; "
                         "flatpak info org.kde.kcalc | grep -E '^ *(ID|Ref):'", 1800)
        check("an app installs from Flathub (KCalc, with its runtime)", "org.kde.kcalc" in out, out[-500:])
        out, _ = vm.run("asimaos 2>/dev/null | head -n 1; flatpak run --command=true org.kde.kcalc && echo started", 300)
        check("…and starts in its sandbox", "started" in out, out)
        vm.shutdown()
    finally:
        vm.kill()


def test_repair(version):
    """The boot partition is gone and an account was deleted; the USB stick repairs it."""
    iso = os.path.join(BUILD, "out", f"asimaos-{version}-x86_64.iso")
    src = os.path.join(BUILD, "out", "asimaos-disk.raw")
    disk = os.path.join(BUILD, "test-disk.raw")
    subprocess.run(["cp", "--reflink=auto", src, disk], check=True)
    print(f"Asima OS repair test: {version}")
    vm = Vm(disk, "test-console-repair.log")
    try:
        print("\nBoot 1: an account, a snapshot, then the damage")
        vm.login()
        vm.run("useradd -m -G wheel asha && echo asha:asha-pw | chpasswd && id asha && echo kept > /home/repair-marker.txt")
        out, rc = vm.run("asimaos snapshot", 300)
        check("snapshot of the files and the settings taken", rc == 0 and "taken" in out, out)
        # The services that keep the ESP open in their own namespace are stopped first, as when
        # the damage comes from outside (another OS, a failing disk).
        vm.run("userdel -r asha; systemctl stop asimaos-maintenanced.service asimaos-updaterd.service; sync")
        out, _ = vm.run("dev=$(blkid -t PARTLABEL=ESP -o device | head -n1); "
                        "ASIMAOS_GUARD=off mkfs.vfat -F 32 \"$dev\" >/dev/null && echo wiped; id asha 2>&1 | head -1")
        check("the boot partition is wiped and the account is gone", "wiped" in out and "no such user" in out, out)
        vm.shutdown()
    finally:
        vm.kill()
    if not all(ok for _, ok in results):
        return
    print("\nBoot 2: the USB stick repairs the disk")
    vm = boot_from_dvd(disk, "test-console-repair2.log", iso)
    try:
        vm.login()
        out, rc = vm.run(f"echo {PASSPHRASE} | /run/initramfs/live/asimaos/asimaos-repair /dev/vda --yes "
                         "--passphrase-stdin --restore-account --install-this-version 2>&1 | tail -n 10", 900)
        check("asimaos-repair installs the USB stick's version, rebuilds the start menu and brings back the settings and accounts",
              "Done." in out and f"Asima OS {version} is installed" in out
              and f"Asima OS {version} (current)" in out and "settings and accounts are back" in out, out)
        vm.shutdown()
    finally:
        vm.kill()
    if not all(ok for _, ok in results):
        return
    print("\nBoot 3: the repaired disk on its own")
    vm = Vm(disk, "test-console-repair3.log")
    try:
        vm.login()
        out, _ = vm.run("grep -o 'asimaos.version=[^ ]*' /proc/cmdline; id -nG asha; cat /home/repair-marker.txt")
        check("it starts again with the USB stick's version, the account is back, the files stayed",
              f"asimaos.version={version}" in out and "wheel" in out and "kept" in out, out)
        vm.shutdown()
    finally:
        vm.kill()


def test_menu_shot(version):
    """The start menu of the disk image, as a picture (build/test-boot-menu-disk.png)."""
    src = os.path.join(BUILD, "out", "asimaos-disk.raw")
    disk = os.path.join(BUILD, "test-disk.raw")
    subprocess.run(["cp", "--reflink=auto", src, disk], check=True)
    vm = Vm(disk, "test-console-menu.log", monitor=True)
    try:
        vm.expect(r"Asima OS [0-9.]+ \(current\)", 120)
        vm.monitor("sendkey down")
        time.sleep(1)
        vm.monitor(f'screendump "{BUILD}/test-boot-menu-disk.png" -f png')
        check("picture of the start menu", os.path.exists(f"{BUILD}/test-boot-menu-disk.png"))
    finally:
        vm.kill()


def wait_for_quiet_disk(limit=300):
    """Until the host disk under build/ has written nothing for 20 s (at most `limit` s). Freeing
    big files keeps Btrfs busy for a while, in bursts, and the firmware's DVD reads time out meanwhile."""
    # Btrfs reports an anonymous device number for its files: ask for the real block device.
    source = subprocess.run(["findmnt", "-n", "-o", "SOURCE", "--target", BUILD],
                            capture_output=True, text=True).stdout.strip().split("[")[0]
    stat = f"/sys/class/block/{os.path.basename(os.path.realpath(source))}/stat" if source else ""
    if not stat or not os.path.exists(stat):
        time.sleep(20)
        return
    last, quiet, end = None, 0, time.time() + limit
    while time.time() < end and quiet < 20:
        with open(stat) as f:
            fields = f.read().split()
        now = (fields[6], fields[8])            # sectors written, I/O in flight
        quiet = quiet + 1 if now == last and fields[8] == "0" else 0
        last = now
        time.sleep(1)


def boot_from_dvd(disk, log_name, iso, attempts=3):
    """A VM started from the ISO, at its login prompt. When the host disk is still busy, the
    firmware's DVD reads fail ("Device Error", then "No bootable option"): let it settle, start again."""
    for attempt in range(1, attempts + 1):
        vm = Vm(disk, log_name, cdrom=iso)
        try:
            seen = vm.expect(r"login: |No bootable option or device was found", 600)
        except BaseException:
            vm.kill()
            raise
        if "No bootable option" not in seen:
            vm.buf = b"login: " + vm.buf          # login() expects the prompt itself
            return vm
        print(f"  (try {attempt}: the firmware couldn't read the DVD; waiting for a quiet disk)")
        vm.kill()
        os.sync()
        wait_for_quiet_disk(600)
    raise RuntimeError("the firmware couldn't read the DVD")


def test_iso(version, plain=False, updates=None):
    iso = os.path.join(BUILD, "out", f"asimaos-{version}-x86_64.iso")
    disk = os.path.join(BUILD, "test-install-disk.raw")
    if os.path.exists(disk):
        os.remove(disk)
    # Freeing a big file (this one, or an earlier test's disk) keeps the host's disk busy for a
    # while; the firmware's DVD reads have short timeouts ("Device Error" in shim), so let it settle.
    os.sync()
    wait_for_quiet_disk()
    # Updates need 10 GB free next to the installed version (docs/architecture.md §8).
    subprocess.run(["truncate", "-s", "64G", disk], check=True)   # sparse; the installer needs 48 GB
    print(f"Asima OS ISO test: {version}  ({os.path.basename(iso)})")
    # Each kind of run keeps its own console logs: test-console-iso-{encrypted,plain,update}-*.log.
    run = "update" if updates else "plain" if plain else "encrypted"

    print("\nBoot 1: live ISO")
    vm = boot_from_dvd(disk, f"test-console-iso-{run}-live.log", iso)
    try:
        vm.login()
        check_common(vm, version)
        m = mount_of(vm, "/var")
        check("/var is tmpfs in live mode", m.split()[1:2] == ["tmpfs"], m)
        # pgrep -f: "asimaos-installer" is longer than the 15 characters a process name keeps.
        out, _ = vm.run("for i in $(seq 90); do pgrep -u live -f asimaos-installer >/dev/null && break; sleep 2; done; "
                        "loginctl list-sessions --no-legend | grep -c ' live '; pgrep -u live -f asimaos-installer | wc -l", 240)
        nums = [int(x) for x in out.split() if x.isdigit()]
        counts = nums[-2:] if len(nums) >= 2 else [0, 0]
        check("live session: 'live' signed in automatically, installer open", min(counts) >= 1, out)
        print(f"\n  -- asimaos-install /dev/vda ({'not encrypted' if plain else 'encrypted'}, account {TEST_USER}) --")
        secrets = f"ASIMAOS_INSTALL_PASSWORD={TEST_PASSWORD} "
        secrets += "" if plain else f"ASIMAOS_INSTALL_PASSPHRASE={INSTALL_PASSPHRASE} "
        options = f"--yes --user {TEST_USER} --fullname 'Test User'" + (" --encrypt no" if plain else "")
        out, rc = vm.run(f"{secrets}/run/initramfs/live/asimaos/asimaos-install /dev/vda {options} 2>&1 | tail -n 15; "
                         "exit_code=${PIPESTATUS[0]}; (exit $exit_code)", 900)
        check("asimaos-install completed", rc == 0 and "is installed on" in out, out)
        vm.shutdown()
    finally:
        vm.kill()
    if not all(ok for _, ok in results):
        return

    print("\nBoot 2: installed disk (no ISO)")
    share = make_update_share(updates) if updates else None
    # The same "PC" every time (firmware variables kept), with a hardware watchdog.
    make_vm = lambda log: Vm(disk, log, passphrase=INSTALL_PASSPHRASE, share=share, monitor=bool(updates),
                             keep_vars=True, watchdog=bool(updates))
    vm = Vm(disk, f"test-console-iso-{run}-installed.log", passphrase=INSTALL_PASSPHRASE, share=share,
            monitor=bool(updates), watchdog=bool(updates))
    try:
        vm.login(TEST_USER, TEST_PASSWORD)
        _, rc = vm.run("grep -q '^root:!' /etc/shadow")
        check("root is locked on the installed system", rc == 0)
        check_installed(vm, version, encrypted=not plain, user=TEST_USER, password=TEST_PASSWORD)
        if updates and all(ok for _, ok in results):
            vm = test_updates(vm, version, *updates, make_vm=make_vm)
        vm.shutdown()
    finally:
        vm.kill()


# ─────────────────────────────────────────────────────────────── updates and going back

def make_update_share(versions):
    """A folder with the update files for the VM (hard links: no copies), shared read-only."""
    folder = os.path.join(BUILD, "test-updates")
    shutil.rmtree(folder, ignore_errors=True)
    os.makedirs(folder)
    for v in versions:
        name = f"asimaos-{v}-x86_64.aosupd"
        os.link(os.path.join(BUILD, "out", name), os.path.join(folder, name))
    return folder


UPDATER = "busctl --timeout=900 call io.github.asimaos.Updater1 /io/github/asimaos/Updater1 io.github.asimaos.Updater1"


def restart(vm, expect_version, user=TEST_USER, password=TEST_PASSWORD, shot=None):
    """Restart and sign in again; returns the version the console showed. With shot=NAME, a
    picture of the start menu is taken on the way (build/test-boot-menu-NAME.png)."""
    vm.send("systemctl reboot\n")
    if shot:
        try:
            vm.expect(r"Asima OS [0-9.]+ \(current\)", 300)   # the menu, mirrored to the serial console
            vm.monitor("sendkey down")                       # any key stops the 3 s countdown
            time.sleep(1)
            vm.monitor(f'screendump "{BUILD}/test-boot-menu-{shot}.png" -f png')
            vm.monitor("sendkey up")
            time.sleep(1)
            vm.monitor(f'screendump "{BUILD}/test-boot-menu-{shot}-current.png" -f png')
            vm.monitor("sendkey ret")
        except TimeoutError:
            print("  (the start menu did not show on the serial console: no picture)")
    seen = vm.banner()
    vm.login(user, password, banner_seen=True)
    out, _ = vm.run("for i in $(seq 150); do systemctl -q is-active boot-complete.target && break; sleep 2; done; "
                    "systemctl is-active asimaos-finish-update.service; grep -o 'asimaos.version=[^ ]*' /proc/cmdline", 400)
    check(f"restarted into {expect_version}", seen == expect_version and f"asimaos.version={expect_version}" in out, out)
    return seen


def apply_update(vm, v):
    out, rc = vm.run(f"{UPDATER} ApplyUpdate h 3 3</mnt/updates/asimaos-{v}-x86_64.aosupd", 900)
    check(f"update to {v} accepted (signature, newer, space)", rc == 0 and "update-" in out, out)
    out, rc = vm.run(esp(f"for i in $(seq 900); do grep -qx 'version  {v}' /boot/loader/entries/asimaos-current+3-0.conf "
                         "2>/dev/null && break; sleep 2; done; sleep 10; ls /boot/loader/entries"), 2000)
    return out


STATUS = ("busctl --json=short call io.github.asimaos.Updater1 /io/github/asimaos/Updater1 io.github.asimaos.Updater1 Status "
          "| jq -r '.data[0]' | jq -r ")


def power_cut(vm, v1, v2, make_vm):
    """The power goes out in the middle of an update; the old version still starts, and
    the same file finishes the update, keeping the parts already written."""
    print(f"\n  -- power cut while updating to {v2} --")
    vm.run(f"({UPDATER} ApplyUpdate h 3 3</mnt/updates/asimaos-{v2}-x86_64.aosupd >/dev/null 2>&1 &)")
    progress = 0.0
    for _ in range(900):
        out, _ = vm.run(STATUS + "'.job.progress // 0'")
        try:
            progress = float(out.split()[-1])
        except (ValueError, IndexError):
            progress = 0.0
        if progress >= 0.35:
            break
        time.sleep(1)
    vm.kill()                                            # the power goes out
    print(f"    power cut at {progress:.0%}")
    time.sleep(3)
    vm = make_vm("test-console-powercut.log")
    vm.login(TEST_USER, TEST_PASSWORD)
    vm.run("mkdir -p /mnt/updates && mount -t 9p -o trans=virtio,version=9p2000.L,ro updates /mnt/updates")
    menu = entries(vm)
    out, _ = vm.run("grep -o 'asimaos.version=[^ ]*' /proc/cmdline")
    check(f"after a power cut at {progress:.0%}: {v1} starts as before, {v2} is not in the start menu",
          f"asimaos.version={v1}" in out and menu.get("current", ("", ""))[1] == v1
          and all(e[1] != v2 for e in menu.values()), f"{out} {menu}")
    return vm


def crash_and_freeze(vm, v2):
    """A kernel crash and a total freeze during trial starts count as failed tries;
    the third try starts fine, the version is marked good, and the crash's log was kept."""
    print(f"\n  -- {v2} on trial: a kernel crash, then a freeze --")
    vm.send("systemctl reboot\n")
    vm.login(TEST_USER, TEST_PASSWORD)                   # try 1
    vm.send("echo c > /proc/sysrq-trigger\n")           # kernel crash; panic=10 restarts
    vm.login(TEST_USER, TEST_PASSWORD)                   # try 2
    vm.send("echo -1 > /proc/sys/kernel/sched_rt_runtime_us; for c in $(seq 0 $(( $(nproc) - 1 ))); do "
            "chrt -f 99 taskset -c $c sh -c 'while :; do :; done' & done\n")   # nothing else gets to run
    vm.login(TEST_USER, TEST_PASSWORD)                   # try 3, after the watchdog's reset
    out, _ = vm.run("tail -c +5 /sys/firmware/efi/efivars/LoaderBootCountPath-4a67b082-0a4c-41cf-b6c7-440b29bb8c4f "
                    "| tr -d '\\0'; echo; for i in $(seq 150); do systemctl -q is-active boot-complete.target && break; "
                    "sleep 2; done; ls /var/lib/systemd/pstore | wc -l", 400)
    check("a crash and a freeze each used up a try: this is the third", "+0-3" in out, out)
    check("the kernel's last messages from the crash were kept (pstore)", out.split()[-1] not in ("0", ""), out)
    vm.run("for i in $(seq 60); do systemctl -q is-active asimaos-finish-update.service && break; sleep 2; done", 200)
    menu = entries(vm)
    check(f"the third try started fine: {v2} is current and marked good",
          menu.get("current") == ("asimaos-current.conf", v2), str(menu))


def test_updates(vm, v1, v2, v3, make_vm):
    print(f"\n  -- update to {v2}, go back, broken update {v3} --")
    vm.run("mkdir -p /mnt/updates && mount -t 9p -o trans=virtio,version=9p2000.L,ro updates /mnt/updates")

    vm = power_cut(vm, v1, v2, make_vm)
    apply_update(vm, v2)
    out, _ = vm.run(STATUS + "'.job.ok, .job.message'")
    check("…the same file finished the update after the power cut", out.split()[:1] == ["true"] and v2 in out, out)
    menu = entries(vm)
    check(f"after the update: {v2} is current (3 tries), {v1} is the backup",
          menu.get("current") == ("asimaos-current+3-0.conf", v2) and menu.get("backup") == ("asimaos-backup.conf", v1), str(menu))
    out, _ = vm.run(esp("grep -h '^title' /boot/loader/entries/*.conf | sort"))
    check("start menu titles: version numbers with current and backup (no alpha/beta)",
          f"Asima OS {v2} (current)" in out and f"Asima OS {v1} (backup)" in out
          and not re.search("alpha|beta", out, re.I), out)

    crash_and_freeze(vm, v2)
    menu = entries(vm)
    check(f"{v2} started fine and is marked good; {v1} stays as backup",
          menu.get("current") == ("asimaos-current.conf", v2) and menu.get("backup") == ("asimaos-backup.conf", v1), str(menu))

    print(f"\n  -- start the backup once ({v1}) --")
    _, rc = vm.run(f"{UPDATER} SetDefaultVersion sb {v1} true")
    check("SetDefaultVersion (once) accepted", rc == 0)
    restart(vm, v1)
    menu = entries(vm)
    check(f"after starting the backup once, {v2} is still current", menu.get("current") == ("asimaos-current.conf", v2), str(menu))

    print(f"\n  -- go back to {v1} for good, then forward to {v2} again --")
    vm.run(f"{UPDATER} SetDefaultVersion sb {v1} false")
    menu = entries(vm)
    check(f"going back: {v1} current, {v2} backup",
          menu.get("current", ("", ""))[1] == v1 and menu.get("backup", ("", ""))[1] == v2, str(menu))
    restart(vm, v1)
    vm.run(f"{UPDATER} SetDefaultVersion sb {v2} false")
    restart(vm, v2)
    menu = entries(vm)
    check(f"forward again: {v2} current, {v1} backup",
          menu.get("current") == ("asimaos-current.conf", v2) and menu.get("backup", ("", ""))[1] == v1, str(menu))

    print(f"\n  -- broken update {v3}: the login screen never starts --")
    vm.run("mkdir -p /mnt/updates; mount -t 9p -o trans=virtio,version=9p2000.L,ro updates /mnt/updates 2>/dev/null; true")
    apply_update(vm, v3)
    menu = entries(vm)
    out, _ = vm.run(f"ls /run/initramfs/slots/ | grep -c {v1}")
    check(f"{v3} is current (3 tries), {v2} the backup, {v1} removed",
          menu.get("current") == ("asimaos-current+3-0.conf", v3) and menu.get("backup", ("", ""))[1] == v2
          and out.strip() == "0", f"{menu} / files of {v1}: {out}")
    vm.send("systemctl reboot\n")
    # A broken start restarts before the console's login banner appears, so the starts are counted
    # from the firmware's own line ("BdsDxe: starting"): three tries of the broken version, then
    # the backup, whose banner ends the wait.
    banner = r"Asima OS (\d{4}\.\d\d\.\d\d\.\d+) \(ttyS0\)"
    seen, starts = [], 0
    while len(seen) < 6:
        out = vm.expect(banner, 1800)
        starts += out.count("BdsDxe: starting")
        seen.append(re.findall(banner, out)[-1])
        print(f"    after {starts} start(s): Asima OS {seen[-1]}")
        if seen[-1] != v3:
            break
    check(f"the broken {v3} was tried 3 times, then {v2} started by itself", seen[-1] == v2 and starts == 4,
          f"{starts} starts, banners {seen}")
    vm.login(TEST_USER, TEST_PASSWORD, banner_seen=True)
    vm.run("for i in $(seq 150); do systemctl -q is-active asimaos-finish-update.service && break; sleep 2; done", 400)
    menu = entries(vm)
    check(f"after the fallback: {v2} is current again, {v3} marked failed",
          menu.get("current") == ("asimaos-current.conf", v2) and menu.get("failed", ("", ""))[1] == v3, str(menu))
    out, _ = vm.run("journalctl -b -1 -o cat -u asimaos-desktop-health.service | grep -m1 'asimaos-health'")
    check("the failed start's log says why (no login screen)", "no login screen" in out, out)
    out, _ = vm.run(f"busctl --json=short call io.github.asimaos.Maintenance1 /io/github/asimaos/Maintenance1 "
                    f"io.github.asimaos.Maintenance1 Status | jq -r '.data[0]' | jq -r '.crashes[] | .version // \"crash\"'")
    check(f"Asima Maintenance lists the failed starts of {v3} and the kernel crash",
          v3 in out and "crash" in out, out)
    return vm


def main():
    version = conf("ASIMAOS_VERSION")
    print(f"(console logs in {BUILD}/test-console-*.log)")
    try:
        if "--iso" in sys.argv:
            updates = None
            if "--update" in sys.argv:
                i = sys.argv.index("--update")
                updates = sys.argv[i + 1:i + 3]
            test_iso(version, plain="--plain" in sys.argv, updates=updates)
        elif "--menu-shot" in sys.argv:
            test_menu_shot(version)
        elif "--secureboot" in sys.argv:
            test_disk(version, secureboot=True)
        elif "--trust" in sys.argv:
            test_trust(version)
        elif "--repair" in sys.argv:
            test_repair(version)
        elif "--reset" in sys.argv:
            test_reset(version)
        elif "--own-keys" in sys.argv:
            test_own_keys(version)
        elif "--rescue" in sys.argv:
            test_rescue_reinstall(version)
        elif "--flathub" in sys.argv:
            test_flathub(version)
        else:
            test_disk(version)
    except TimeoutError as e:
        print(f"\n\033[31mERROR:\033[0m {e}")
        results.append(("boot sequence", False))
    finally:
        # A test that stopped early must not leave a VM (and its memory) behind.
        for vm in LIVE_VMS:
            vm.kill()
    passed = sum(ok for _, ok in results)
    print(f"\nResult: {passed}/{len(results)} checks passed")
    sys.exit(0 if results and passed == len(results) else 1)


if __name__ == "__main__":
    main()
