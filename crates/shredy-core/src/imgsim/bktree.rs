//! ハミング距離 BK-tree（仕様 §3.12 クラスタリング）。
//!
//! 全 pHash を投入し、各点から半径 `maxBits` の近傍探索で距離 ≤ maxBits のエントリを
//! 列挙する。総当たり O(N²) を平均 O(N log N) に近づける。距離 0（同一 pHash）は
//! 同一ノードへ集約する。

use std::collections::HashMap;
use std::collections::hash_map::Entry;

/// 2 つの pHash のハミング距離（0〜64, 仕様 §3.12）。
#[must_use]
pub fn hamming(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

/// BK-tree のノード。同一 pHash を持つエントリは `indices` に集約する。
struct Node {
    hash: u64,
    indices: Vec<usize>,
    children: HashMap<u32, Node>,
}

impl Node {
    fn new(hash: u64, index: usize) -> Self {
        Self {
            hash,
            indices: vec![index],
            children: HashMap::new(),
        }
    }
}

/// ハミング距離空間の BK-tree。
#[derive(Default)]
pub struct BkTree {
    root: Option<Node>,
}

impl BkTree {
    /// 空の木を作る。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// `hash` を持つエントリ `index` を挿入する。
    pub fn insert(&mut self, hash: u64, index: usize) {
        match &mut self.root {
            None => self.root = Some(Node::new(hash, index)),
            Some(root) => insert_node(root, hash, index),
        }
    }

    /// `query` から距離 ≤ `radius` のエントリ index をすべて `out` へ集める。
    pub fn search(&self, query: u64, radius: u32, out: &mut Vec<usize>) {
        if let Some(root) = &self.root {
            search_node(root, query, radius, out);
        }
    }
}

/// 距離 1 の連鎖などで木の高さは最悪 O(N) になり得る（最大距離 64 では有界でない）。
/// スタックオーバーフローを避けるため再帰ではなくループで降りる。距離 0（同一 pHash）は
/// 同一ノードへ集約する。
fn insert_node(root: &mut Node, hash: u64, index: usize) {
    let mut node = root;
    loop {
        let d = hamming(node.hash, hash);
        if d == 0 {
            node.indices.push(index);
            return;
        }
        // 子が無ければ作って終了、あれば into_mut() で降りる（再帰なしの木降下）。
        match node.children.entry(d) {
            Entry::Occupied(e) => node = e.into_mut(),
            Entry::Vacant(e) => {
                e.insert(Node::new(hash, index));
                return;
            }
        }
    }
}

/// 木の高さは最悪 O(N) になり得るため、明示スタックで反復探索する（再帰のスタック
/// オーバーフロー回避）。`out` への収集順は問わない（呼び出し側はソート/集合で扱う）。
fn search_node(root: &Node, query: u64, radius: u32, out: &mut Vec<usize>) {
    let mut stack: Vec<&Node> = vec![root];
    while let Some(node) = stack.pop() {
        let d = hamming(node.hash, query);
        if d <= radius {
            out.extend_from_slice(&node.indices);
        }
        // 三角不等式: 子の親距離 cd が [d-radius, d+radius] のみ探索する。
        let lo = d.saturating_sub(radius);
        let hi = d.saturating_add(radius);
        for (&cd, child) in &node.children {
            if cd >= lo && cd <= hi {
                stack.push(child);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sorted_search(tree: &BkTree, query: u64, radius: u32) -> Vec<usize> {
        let mut out = Vec::new();
        tree.search(query, radius, &mut out);
        out.sort_unstable();
        out
    }

    #[test]
    fn hamming_counts_differing_bits() {
        assert_eq!(hamming(0b0000, 0b0000), 0);
        assert_eq!(hamming(0b1010, 0b0011), 2);
        assert_eq!(hamming(u64::MAX, 0), 64);
    }

    #[test]
    fn radius_zero_returns_only_exact_matches() {
        let mut tree = BkTree::new();
        tree.insert(0b0000, 0);
        tree.insert(0b0000, 1); // 同一ハッシュは集約
        tree.insert(0b0001, 2);
        assert_eq!(sorted_search(&tree, 0b0000, 0), vec![0, 1]);
    }

    #[test]
    fn radius_search_matches_bruteforce() {
        let hashes = [0b0000u64, 0b0001, 0b0011, 0b0111, 0b1111, 0b1000];
        let mut tree = BkTree::new();
        for (i, &h) in hashes.iter().enumerate() {
            tree.insert(h, i);
        }
        for radius in 0..=4u32 {
            for &q in &hashes {
                let mut brute: Vec<usize> = hashes
                    .iter()
                    .enumerate()
                    .filter(|&(_, &h)| hamming(h, q) <= radius)
                    .map(|(i, _)| i)
                    .collect();
                brute.sort_unstable();
                assert_eq!(
                    sorted_search(&tree, q, radius),
                    brute,
                    "radius={radius} q={q:b}"
                );
            }
        }
    }
}
