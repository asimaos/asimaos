# How Asima OS is run

Asima OS is a small project with one maintainer, built in the open. This page says who decides,
how changes get in, how a release is made, and how that can grow.

## Who decides

The maintainer decides: what Asima OS does, what goes into a version, and when a version is
released. Every decision and its reason is written down in [`docs/decisions.md`](docs/decisions.md),
which wins wherever other documents disagree. Questions and ideas are welcome in the issues (and in
the discussions, once they are open); a decision that changes how something works is talked about
first, then recorded there.

## How changes get in

Work happens in the maintainer's private working repository. The public repositories, `asimaos`,
`asimaos-hub` and `asimaos-website`, are published from it, each commit with its message, author and
date, and they only ever move forward. A pull request here is applied there with its author kept, and
appears here with the next publish; the pull request is then closed with a note. Contributions are
under the GPL-3.0-or-later, with no sign-off line or extra agreement.

## How a version is released

1. The version is built from a pinned Arch Linux snapshot, and every automated test runs: the
   services' tests, the apps' screens, and the virtual machine suites (installing, updating, going
   back, Secure Boot, TPM unlock, the repair tools).
2. The maintainer tests it on real PCs.
3. The release key signs the update file and the checksums of the ISO. The key stays with the
   maintainer, in an encrypted copy, and is never in any repository.
4. The release goes to the download page and the news, and Asima Hub tells people about it.

[`docs/status.md`](docs/status.md) shows every test and its result.

## Becoming a maintainer

Someone who has helped for a good while, with changes, tests on real hardware or answers to other
people, may be asked to become a maintainer: they then review and merge changes and look after the
issues. The signing keys stay with the project's maintainer.

## Conduct

Everyone here follows the [code of conduct](CODE_OF_CONDUCT.md).

## Changes to this page

The maintainer changes this page; each change is in the repository's history.
