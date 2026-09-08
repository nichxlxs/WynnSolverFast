//! Collision-safe memo keys for subtree and last-slot objective ceilings.
//!
//! Small pools keep the original u64 representation. Wider pools use full
//! offsets and the complete remaining band budget: truncating either can
//! reuse a ceiling from a different (possibly weaker) subtree and false-prune.

use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum CeilingKind {
    Subtree,
    Cluster,
    SuperCluster,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct WideKey {
    kind: CeilingKind,
    depth: usize,
    prefix: [usize; 8],
    // Subtree: selected item offset. Clusters: cluster index.
    index: usize,
    h_child: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MemoKey {
    Packed(u64),
    Wide(WideKey),
}

pub(crate) enum BoundMemo {
    Packed(HashMap<u64, f64>),
    Wide(HashMap<WideKey, f64>),
}

impl BoundMemo {
    pub(crate) fn new(wide: bool) -> Self {
        if wide { Self::Wide(HashMap::new()) } else { Self::Packed(HashMap::new()) }
    }

    #[inline]
    pub(crate) fn key(
        &self, kind: CeilingKind, depth: usize, offsets: &[usize; 8],
        index: usize, h_child: i64,
    ) -> MemoKey {
        debug_assert!(depth < 8);
        match self {
            Self::Wide(_) => {
                // Ignore stale offsets beyond the active prefix. Different
                // band visits to the same state must share the same key.
                let mut prefix = [0; 8];
                prefix[..depth].copy_from_slice(&offsets[..depth]);
                MemoKey::Wide(WideKey {
                    kind, depth, prefix, index,
                    h_child: if kind == CeilingKind::Subtree { h_child } else { 0 },
                })
            }
            Self::Packed(_) => {
                // Only constructed for pools <128. Subtree bounds run before
                // the last depth, hence <=6; the maximum total rank is 1008.
                debug_assert!(offsets[..depth].iter().all(|&o| o < 128));
                let key = match kind {
                    CeilingKind::Subtree => {
                        debug_assert!(depth < 7 && index < 128);
                        debug_assert!((0..=2047).contains(&h_child));
                        let mut key = (depth as u64) << 60 | h_child as u64;
                        for (d, &offset) in offsets[..depth].iter().enumerate() {
                            key |= (offset as u64) << (11 + d * 7);
                        }
                        key | (index as u64) << (11 + depth * 7)
                    }
                    CeilingKind::Cluster | CeilingKind::SuperCluster => {
                        debug_assert!(index < 2048);
                        let tag = if kind == CeilingKind::Cluster { 0xFu64 } else { 0xEu64 };
                        let mut key = tag << 60 | (index as u64) << 49;
                        for (d, &offset) in offsets[..depth].iter().enumerate() {
                            key |= (offset as u64) << (d * 7);
                        }
                        key
                    }
                };
                MemoKey::Packed(key)
            }
        }
    }

    #[inline]
    pub(crate) fn get(&self, key: &MemoKey) -> Option<&f64> {
        match (self, key) {
            (Self::Packed(map), MemoKey::Packed(key)) => map.get(key),
            (Self::Wide(map), MemoKey::Wide(key)) => map.get(key),
            _ => unreachable!("memo key representation changed during search"),
        }
    }

    #[inline]
    pub(crate) fn insert(&mut self, key: MemoKey, value: f64) {
        match (self, key) {
            (Self::Packed(map), MemoKey::Packed(key)) => { map.insert(key, value); }
            (Self::Wide(map), MemoKey::Wide(key)) => { map.insert(key, value); }
            _ => unreachable!("memo key representation changed during search"),
        }
    }

    pub(crate) fn len(&self) -> usize {
        match self { Self::Packed(map) => map.len(), Self::Wide(map) => map.len() }
    }

    pub(crate) fn clear(&mut self) {
        match self { Self::Packed(map) => map.clear(), Self::Wide(map) => map.clear() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn wide_keys_separate_offsets_and_namespaces() {
        let mut memo = BoundMemo::new(true);
        let mut keys = HashSet::new();
        for &a in &[0, 1, 126, 127, 128, 129, 255, 256, 257, 65536] {
            for &b in &[0, 1, 127, 128, 129, 256] {
                for &kind in &[CeilingKind::Subtree, CeilingKind::Cluster, CeilingKind::SuperCluster] {
                    let mut prefix = [0; 8];
                    prefix[0] = a;
                    let key = memo.key(kind, 1, &prefix, b, 4000);
                    let MemoKey::Wide(wide) = key else { panic!("expected wide key") };
                    assert!(keys.insert(wide), "collision: {wide:?}");
                    memo.insert(key, (a * 1000 + b) as f64);
                    assert_eq!(memo.get(&key), Some(&((a * 1000 + b) as f64)));
                }
            }
        }
        assert_eq!(memo.len(), keys.len());
    }

    #[test]
    fn wide_subtree_rank_is_not_clamped_or_truncated() {
        let memo = BoundMemo::new(true);
        let mut keys = HashSet::new();
        for h_child in [-1, 0, 1, 2046, 2047, 2048, 4095, 4096, i64::MAX] {
            let MemoKey::Wide(key) = memo.key(CeilingKind::Subtree, 6, &[129; 8], 256, h_child)
                else { panic!("expected wide key") };
            assert!(keys.insert(key));
        }
    }

    #[test]
    fn stale_suffixes_are_ignored_but_prefix_depth_is_preserved() {
        let memo = BoundMemo::new(true);
        let mut stale = [999; 8];
        stale[0] = 128;
        let mut clean = [0; 8];
        clean[0] = 128;
        for kind in [CeilingKind::Subtree, CeilingKind::Cluster, CeilingKind::SuperCluster] {
            assert_eq!(memo.key(kind, 1, &clean, 3, 20), memo.key(kind, 1, &stale, 3, 20));
            assert_ne!(memo.key(kind, 1, &clean, 3, 20), memo.key(kind, 2, &clean, 3, 20));
        }
    }

    #[test]
    fn packed_keys_are_injective_in_supported_small_pool_domain() {
        let memo = BoundMemo::new(false);
        let mut keys = HashSet::new();
        for &depth in &[0, 1, 6] {
            for &offset in &[0, 1, 126, 127] {
                for &h in &[0, 1, 127, 2047] {
                    let MemoKey::Packed(key) = memo.key(CeilingKind::Subtree, depth, &[127; 8], offset, h)
                        else { panic!("expected packed key") };
                    assert!(keys.insert(key));
                }
            }
        }
        for kind in [CeilingKind::Cluster, CeilingKind::SuperCluster] {
            for index in [0, 1, 126, 127] {
                let MemoKey::Packed(key) = memo.key(kind, 7, &[127; 8], index, 0)
                    else { panic!("expected packed key") };
                assert!(keys.insert(key));
            }
        }
    }
}
