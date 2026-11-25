//! The two versions on the disk share the blocks they have in common (D55). Right after an update,
//! while the slots are writable in this service's own mount namespace, every run of blocks in the
//! new system image that the running version's image has too is shared with it (FIDEDUPERANGE), so
//! two versions take little more room than one. The kernel compares the bytes itself before it
//! shares anything, so blocks that only looked alike stay as they are, and both images stay
//! exactly what dm-verity expects.
//!
//! The images are EROFS without compression: each file's data starts at a 4 KiB block, so an
//! unchanged file is the same run of 4 KiB blocks in both, wherever it moved to. Blocks are
//! matched by a hash of their content, runs of at least 64 KiB are shared, at most 16 MiB per call
//! (Btrfs's limit). A failure only means less room is saved.

use anyhow::Result;
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::hash::{Hash, Hasher};
use std::io::{BufReader, Read};
use std::os::unix::io::AsRawFd;
use std::path::Path;

const BLOCK: u64 = 4096;
/// Shorter runs aren't worth an extent of their own (16 blocks: 64 KiB).
const MIN_RUN: u64 = 16;
/// Btrfs shares at most 16 MiB per call.
const MAX_CALL: u64 = 16 << 20;
/// _IOWR(0x94, 54, struct file_dedupe_range)
const FIDEDUPERANGE: libc::c_ulong = 0xC018_9436;

/// struct file_dedupe_range with its one struct file_dedupe_range_info.
#[repr(C)]
struct DedupeRange {
    src_offset: u64,
    src_length: u64,
    dest_count: u16,
    reserved1: u16,
    reserved2: u32,
    dest_fd: i64,
    dest_offset: u64,
    bytes_deduped: u64,
    status: i32,
    reserved: u32,
}

/// A block's content as a number; 0 for a block of zeros, which is never shared.
fn block_hash(b: &[u8]) -> u64 {
    if b.iter().all(|&x| x == 0) {
        return 0;
    }
    let mut h = std::collections::hash_map::DefaultHasher::new();
    b.hash(&mut h);
    h.finish().max(1)
}

/// The hash of each whole block of a file, in order.
fn hashes(f: &File) -> Result<Vec<u64>> {
    let mut r = BufReader::with_capacity(1 << 20, f);
    let mut buf = [0u8; BLOCK as usize];
    let mut out = Vec::new();
    loop {
        match r.read_exact(&mut buf) {
            Ok(()) => out.push(block_hash(&buf)),
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(out),
            Err(e) => return Err(e.into()),
        }
    }
}

/// The runs of blocks the new image has in common with the old one: (old block, new block,
/// length), each at least MIN_RUN blocks long.
pub fn runs(old: &[u64], new: impl IntoIterator<Item = u64>) -> Vec<(u64, u64, u64)> {
    let mut first: HashMap<u64, u64> = HashMap::with_capacity(old.len());
    for (i, &h) in old.iter().enumerate() {
        if h != 0 {
            first.entry(h).or_insert(i as u64);
        }
    }
    let mut out = Vec::new();
    let (mut o0, mut n0, mut len) = (0u64, 0u64, 0u64);
    let mut flush = |o0: u64, n0: u64, len: u64| {
        if len >= MIN_RUN {
            out.push((o0, n0, len));
        }
    };
    for (n, h) in new.into_iter().enumerate() {
        let n = n as u64;
        // The run goes on while the old image has the same block next.
        if len > 0 && h != 0 && old.get((o0 + len) as usize) == Some(&h) {
            len += 1;
            continue;
        }
        flush(o0, n0, len);
        len = 0;
        if let Some(&o) = first.get(&h).filter(|_| h != 0) {
            (o0, n0, len) = (o, n, 1);
        }
    }
    flush(o0, n0, len);
    out
}

/// Shares `len` bytes of `dst` at `dst_off` with `src` at `src_off`; returns the bytes shared.
fn share_range(src: &File, dst: &File, src_off: u64, dst_off: u64, len: u64) -> u64 {
    let (mut done, mut off) = (0, 0);
    while off < len {
        let n = (len - off).min(MAX_CALL);
        let mut r = DedupeRange {
            src_offset: src_off + off,
            src_length: n,
            dest_count: 1,
            reserved1: 0,
            reserved2: 0,
            dest_fd: i64::from(dst.as_raw_fd()),
            dest_offset: dst_off + off,
            bytes_deduped: 0,
            status: 0,
            reserved: 0,
        };
        // SAFETY: FIDEDUPERANGE with one destination: the kernel reads the header and the one
        // info entry that follows it in this struct, and writes its result into that entry.
        let rc = unsafe { libc::ioctl(src.as_raw_fd(), FIDEDUPERANGE, &mut r) };
        if rc == 0 && r.status == 0 {
            done += r.bytes_deduped;
        } else if rc != 0 {
            // Not supported here (not Btrfs): nothing more to try.
            return done;
        }
        off += n;
    }
    done
}

/// After an update: the new version's system image shares what it has in common with the running
/// version's. Returns the bytes now shared.
pub fn share(slots: &Path, running: &str, new: &str) -> Result<u64> {
    if running == new {
        return Ok(0);
    }
    let old = File::open(slots.join(format!("usr_{running}.erofs")))?;
    let dst = OpenOptions::new().read(true).write(true).open(slots.join(format!("usr_{new}.erofs")))?;
    let old_hashes = hashes(&old)?;
    let new_hashes = hashes(&dst)?;
    let mut shared = 0;
    for (o, n, len) in runs(&old_hashes, new_hashes) {
        shared += share_range(&old, &dst, o * BLOCK, n * BLOCK, len * BLOCK);
    }
    Ok(shared)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_moved_runs() {
        // The old image: blocks 1..=40; the new one: a new block, old 21..=40, two new ones,
        // old 1..=17, zeros, old 30..=33 (too short to share).
        let old: Vec<u64> = (1..=40).collect();
        let mut new = vec![900];
        new.extend(21..=40);
        new.extend([901, 902]);
        new.extend(1..=17);
        new.extend([0, 0]);
        new.extend(30..=33);
        assert_eq!(runs(&old, new), [(20, 1, 20), (0, 23, 17)]);
        // Zero blocks are never shared.
        assert!(runs(&[0; 40], vec![0; 40]).is_empty());
    }

    #[test]
    fn hashes_blocks() {
        assert_eq!(block_hash(&[0u8; 4096]), 0);
        assert_ne!(block_hash(&[1u8; 4096]), block_hash(&[2u8; 4096]));
        assert_ne!(block_hash(&[1u8; 4096]), 0);
    }
}
