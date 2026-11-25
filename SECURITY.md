# Security

Asima OS protects people's computers with signed updates, Secure Boot, disk encryption and
services that act only after polkit says yes. A weakness in any of them matters, so please report
it privately and give us time to fix it before it becomes public.

## Reporting a problem

**Don't open a public issue.** Use GitHub's private report instead: on
[github.com/asimaos/asimaos](https://github.com/asimaos/asimaos), open the
**Security** tab and choose **Report a vulnerability**. Only the maintainers see it.

Please tell us:

- the version (Asima Updater shows it), or the commit if you built it yourself;
- what you found and what an attacker could do with it;
- how to reproduce it, as exactly as you can.

You'll hear back within a week. Once a fix has shipped, the report can become public, with your
name on it if you like.

## What counts

Anything that weakens the update signatures or lets an update in that wasn't signed; the start-up
chain (Secure Boot, the signed start-up image, dm-verity); disk encryption and TPM unlock; the
Asima OS password dialog, fingerprint and face approval; a service that does root work without
polkit's yes; signed system extensions; Asima Hub's checks of its feed, downloads and USB stick;
Asima App Center's checks of what it downloads; the build's handling of keys.

## Supported versions

The newest version. Asima OS updates as a whole, so a fix ships as a new version.
