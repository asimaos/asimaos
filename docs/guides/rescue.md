# Asima Rescue

Asima Rescue is a small system in the start menu that runs entirely in memory. It repairs Asima OS without starting it.

**Start it:** the start menu shows for three seconds when the computer starts; press an arrow key
to keep it on screen and choose **Asima Rescue**. When the current version fails three starts in a
row, the backup starts by itself; Rescue is there for when that isn't enough.

| Tool | What it does |
| --- | --- |
| Unlock the disk | With the passphrase or the recovery key; the TPM never unlocks it here |
| Start the previous version | The backup, for the next start only |
| Check and repair the disk | The file system, both versions, the boot files, free space |
| Restore my files | Your home folder back to a snapshot; today's files are kept as a snapshot first |
| Reinstall Asima OS | From an update file on a USB stick. Your files, apps and settings stay. An older version is allowed after a warning |
| Secure Boot and unlock | Approve the Asima OS key again; turn automatic unlock off |
| Factory reset | Asima OS starts fresh with a new account |
| Erase the whole disk | Everything on it, Asima OS included |

If even Asima Rescue doesn't start, start the computer from the Asima OS USB stick and choose
**Repair this computer**. It can also install the USB stick's version of Asima OS (answer yes
when it asks): your files, apps and settings stay. Use that when updates stopped working, for
example after the project changed its release key.
