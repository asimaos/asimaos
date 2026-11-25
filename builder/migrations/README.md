# Migration steps

One-time steps that bring an installed computer's settings (`/etc`, `/var`, the ESP, the
subvolumes) into the shape a new version needs (D52, [architecture §8](../../docs/architecture.md#8-updates-health-checks-and-automatic-rollback)).

- **Name:** `<version>-<what-it-does>.sh`, where `<version>` is the first version that needs it,
  for example `2027.01.12.1-libvirt-networks.sh`. Steps run in version order, then by name.
- **Packed:** every update file carries the steps of its window: newer than its `MIN_FROM` (one
  year before the release, `ASIMAOS_UPDATE_WINDOW` in `builder/asimaos.conf`) and not newer than
  its own version. A computer that skipped versions runs all the steps it missed, each once.
- **Run:** by `asimaos-migrate.service` as root, early in the new version's first start, with
  bash. A step that fails stops the start's migrations; the start menu then falls back to the
  version before, and the settings go back to the snapshot taken before the update.
- **Rules:** a step must keep the previous version working (going back reuses the same `/etc`
  and `/var`), may run on a computer whose settings were changed by hand, and must do nothing
  when there is nothing to do. It uses only tools every version has (bash, coreutils, util-linux,
  btrfs-progs). Fresh installations never run steps: they start in the new shape.
