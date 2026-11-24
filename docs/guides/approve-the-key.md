# The blue screen: approving the Asima OS key

Asima OS signs everything that starts the computer with its own key. Secure Boot only starts what
it trusts, so the first time, you tell it once that the Asima OS key is yours to trust. That happens
on a blue screen called **"Perform MOK management"** (MokManager, part of shim).

## When it appears

- At the first start after installing, if Secure Boot is on.
- After you chose "Approve the Asima OS key" in Asima Maintenance → Security, or in Asima Rescue.

## What to do

1. Asima Maintenance (or the installer) shows a **code of digits**. Note it.
2. Restart. The blue screen appears for ten seconds: press a key to stay on it.
3. Choose **Enroll MOK**, then **Continue**, then **Yes**.
4. Type the code and press Enter. It's digits only, so any keyboard works.
5. Choose **Reboot**.

If you miss the screen, nothing is lost: approve the key again in Asima Maintenance → Security.

## If Secure Boot refuses to start Asima OS

The key is also kept on the boot partition. On the blue screen choose **Enroll key from disk**, then
`EFI/BOOT/asimaos.der`, and confirm. Or switch Secure Boot off in the firmware settings for one
start and approve the key from Asima Maintenance.
