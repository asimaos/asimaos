# Contributing to Asima OS

Thank you for helping. Asima OS is an image-based Arch Linux + KDE Plasma system: every version is
one signed image, updates are one file, and the person at the computer never has to repair it.

## Before you start

- Read [`docs/decisions.md`](docs/decisions.md). It holds the project's decisions and wins wherever
  other documents disagree. Changing one of them is a conversation first, not a pull request.
- [`docs/architecture.md`](docs/architecture.md) explains how the system fits together,
  [`apps/DESIGN.md`](apps/DESIGN.md) how the apps look and talk to their services, and
  [`docs/status.md`](docs/status.md) what works today.
- Translations are welcome: `apps/i18n/asimaos_<language>.ts`, opened with Qt Linguist.
- This repository is published from the maintainer's private working repository,
  so a pull request isn't merged here directly: its change is applied there, keeping you as its
  author, and appears here with the next push; the pull request is then closed with a note.

## Building and testing

[`docs/building.md`](docs/building.md) has everything. In short, all of it runs in rootless Podman
containers, without sudo:

```bash
apps/build.sh services-test        # the services' tests and each app against its service
apps/build.sh screenshots [APP]    # every screen, light and dark, in apps/screenshots/
./build.sh                         # the OS image, ISO and update file (about an hour)
python3 testbed/boot-test.py         # the image in QEMU (see docs/building.md for the modes)
```

Run one container at a time. A change to an app is checked from its screenshots, light and dark.

## The rules that keep it working

- **Honest about limits.** No control that does nothing; a feature that isn't ready says so.
- **Plain words first** in everything people read: "the start menu", "go back to this version".
  British spelling. Technical detail goes into subtitles.
- **One app, one job; one service per app** (D27). Apps never need root; their service does the
  root work behind polkit.
- **Secrets never on a command line**, only through stdin or the environment.
- **Rust:** `cargo fmt` and `cargo clippy` stay clean.
- **Docs change with behaviour, and each fact lives in one place:** how the system works in
  `docs/architecture.md`, the apps and their services in `apps/DESIGN.md`, tests and their results
  in `docs/status.md`, plans in `docs/roadmap.md`, what changed in the CHANGELOG. Other documents
  link there instead of repeating it.

## Never

- Commit keys. The release and Secure Boot keys of a build (`build/keys`) are never in this
  repository; a guard refuses them.
- Add a package to the image without a reason that fits D28 (only what must run natively).

## Reporting a security problem

Please don't open a public issue: [SECURITY.md](SECURITY.md) says how to report it privately.
