//! Compiled instruction cache: a two-level table over the 24-bit byte address space,
//! one 256-byte page per leaf, so a lookup is two indexed loads and dropping a code page
//! (CodeCache.dropped) drops its leaf.

use std::rc::Rc;

use crate::core::Compiled;

const PAGE_BITS: u32 = 8;
const PAGE: usize = 1 << PAGE_BITS;
const PAGES: usize = 1 << (24 - PAGE_BITS);

type Leaf = Box<[Option<Rc<Compiled>>; PAGE]>;

/// Most compiled instructions the cache holds: an engineering cap against a runaway
/// decode, not architectural.
pub const MAX_CACHED_INSNS: usize = 1 << 20;

pub struct ICache {
    pages: Vec<Option<Leaf>>,
    len: usize,
}

impl ICache {
    pub fn new() -> ICache {
        ICache { pages: (0..PAGES).map(|_| None).collect(), len: 0 }
    }

    /// Compiled instructions held.
    pub fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub fn get(&self, pc: u32) -> Option<&Rc<Compiled>> {
        let pc = pc as usize & 0xFFFFFF;
        self.pages[pc >> PAGE_BITS].as_ref()?[pc & (PAGE - 1)].as_ref()
    }

    pub fn insert(&mut self, pc: u32, c: Rc<Compiled>) {
        let pc = pc as usize & 0xFFFFFF;
        let leaf = self.pages[pc >> PAGE_BITS].get_or_insert_with(|| Box::new(std::array::from_fn(|_| None)));
        if leaf[pc & (PAGE - 1)].replace(c).is_none() {
            self.len += 1;
        }
    }
}

/// Hasher for sets of code addresses (stop pcs, poll loops), looked up on every step:
/// one multiply instead of SipHash. Addresses are not attacker controlled.
#[derive(Default)]
pub struct PcHasher(u64);

impl std::hash::Hasher for PcHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0 << 8 | b as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        }
    }
    fn write_u32(&mut self, v: u32) {
        self.0 = (v as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    }
}

pub type PcSet = std::collections::HashSet<u32, std::hash::BuildHasherDefault<PcHasher>>;

/// A PcSet behind a 64-bit Bloom filter: most pcs are rejected with one AND.
#[derive(Default)]
pub struct PcFilter {
    set: PcSet,
    bloom: u64,
}

impl PcFilter {
    pub fn new(set: PcSet) -> PcFilter {
        let bloom = set.iter().fold(0, |b, &pc| b | Self::bit(pc));
        PcFilter { set, bloom }
    }

    #[inline]
    fn bit(pc: u32) -> u64 {
        1 << ((pc ^ (pc >> 6) ^ (pc >> 12)) & 63)
    }

    #[inline]
    pub fn contains(&self, pc: u32) -> bool {
        self.bloom & Self::bit(pc) != 0 && self.set.contains(&pc)
    }
}
