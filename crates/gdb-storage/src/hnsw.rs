use ahash::RandomState;
use gdb_core::VertexId;
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};

#[derive(Clone, Debug)]
struct HnswNode {
    vid: VertexId,
    vector: Vec<f32>,
    norm: f32,
    deleted: bool,
    layers: Vec<Vec<u32>>,
}

#[derive(Clone, Copy, Debug)]
struct DistCandidate {
    dist: f32,
    idx: u32,
}

impl PartialEq for DistCandidate {
    fn eq(&self, other: &Self) -> bool {
        self.idx == other.idx && self.dist.to_bits() == other.dist.to_bits()
    }
}
impl Eq for DistCandidate {}

// Min-heap ordering (smallest distance first)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MinCandidate(DistCandidate);

impl Ord for MinCandidate {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .0
            .dist
            .partial_cmp(&self.0.dist)
            .unwrap_or(Ordering::Equal)
            .then_with(|| other.0.idx.cmp(&self.0.idx))
    }
}

impl PartialOrd for MinCandidate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

// Max-heap ordering (largest distance first, for bounding ef candidates)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MaxCandidate(DistCandidate);

impl Ord for MaxCandidate {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0
            .dist
            .partial_cmp(&other.0.dist)
            .unwrap_or(Ordering::Equal)
            .then_with(|| self.0.idx.cmp(&other.0.idx))
    }
}

impl PartialOrd for MaxCandidate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Hierarchical Navigable Small World (HNSW) Graph Index for sub-millisecond ANN vector search.
#[derive(Clone, Debug)]
pub struct HnswIndex {
    pub dim: usize,
    pub m: usize,
    pub m0: usize,
    pub ef_construction: usize,
    pub ef_search: usize,
    ml: f64,
    nodes: Vec<HnswNode>,
    vid_to_idx: HashMap<u64, u32, RandomState>,
    entry_point: Option<u32>,
    max_level: usize,
    active_count: usize,
}

impl HnswIndex {
    pub fn new(dim: usize, m: usize, ef_construction: usize, ef_search: usize) -> Self {
        let m = m.max(4);
        let m0 = m * 2;
        let ml = 1.0 / (m as f64).ln();
        Self {
            dim,
            m,
            m0,
            ef_construction: ef_construction.max(16),
            ef_search: ef_search.max(32),
            ml,
            nodes: Vec::new(),
            vid_to_idx: HashMap::with_hasher(RandomState::new()),
            entry_point: None,
            max_level: 0,
            active_count: 0,
        }
    }

    pub fn with_defaults(dim: usize) -> Self {
        Self::new(dim, 16, 64, 64)
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.active_count
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.active_count == 0
    }

    #[inline]
    fn compute_norm(v: &[f32]) -> f32 {
        v.iter().map(|x| x * x).sum::<f32>().sqrt()
    }

    #[inline]
    fn dot_product(a: &[f32], b: &[f32]) -> f32 {
        a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
    }

    #[inline]
    fn l2_dist_sq(a: &[f32], b: &[f32]) -> f32 {
        a.iter()
            .zip(b.iter())
            .map(|(x, y)| {
                let d = x - y;
                d * d
            })
            .sum()
    }

    /// Internal distance metric (smaller is closer) used during graph construction & navigation.
    #[inline]
    fn distance(&self, query: &[f32], query_norm: f32, node_idx: u32, metric: &str) -> f32 {
        let node = &self.nodes[node_idx as usize];
        match metric {
            "l2" | "euclidean" => Self::l2_dist_sq(query, &node.vector),
            "dot" | "dot_product" => -Self::dot_product(query, &node.vector),
            _ => {
                // Default: cosine distance = 1.0 - cosine_similarity
                if query_norm == 0.0 || node.norm == 0.0 {
                    1.0
                } else {
                    let cos = Self::dot_product(query, &node.vector) / (query_norm * node.norm);
                    1.0 - cos.clamp(-1.0, 1.0)
                }
            }
        }
    }

    /// Public similarity score matching GDB's `vector.similaritySearch` semantics (larger is better).
    #[inline]
    fn similarity_score(&self, query: &[f32], query_norm: f32, node_idx: u32, metric: &str) -> f32 {
        let node = &self.nodes[node_idx as usize];
        match metric {
            "l2" | "euclidean" => {
                let dist_sq = Self::l2_dist_sq(query, &node.vector);
                1.0 / (1.0 + dist_sq.sqrt())
            }
            "dot" | "dot_product" => Self::dot_product(query, &node.vector),
            _ => {
                if query_norm == 0.0 || node.norm == 0.0 {
                    0.0
                } else {
                    Self::dot_product(query, &node.vector) / (query_norm * node.norm)
                }
            }
        }
    }

    /// Deterministic level generator based on vertex ID and insertion order.
    fn sample_level(&self, vid: u64, seq: usize) -> usize {
        let mut z = vid
            .wrapping_add((seq as u64).wrapping_mul(0x9E3779B97F4A7C15))
            .wrapping_add(0x9E3779B97F4A7C15);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z = z ^ (z >> 31);
        let u = ((z >> 11) as f64) * (1.0 / ((1u64 << 53) as f64));
        let clamped = u.max(1e-9);
        ((-clamped.ln() * self.ml).floor() as usize).min(12)
    }

    /// Inserts or updates a vertex vector in the HNSW graph.
    pub fn insert(&mut self, vid: VertexId, vector: Vec<f32>) {
        if self.dim == 0 && !vector.is_empty() {
            self.dim = vector.len();
        }
        if vector.len() != self.dim || self.dim == 0 {
            return;
        }

        let norm = Self::compute_norm(&vector);

        if let Some(&existing_idx) = self.vid_to_idx.get(&vid.0) {
            let node = &mut self.nodes[existing_idx as usize];
            if node.deleted {
                self.active_count += 1;
                node.deleted = false;
            }
            node.vector = vector;
            node.norm = norm;
            if self.entry_point.is_none() {
                self.entry_point = Some(existing_idx);
            }
            return;
        }

        let new_idx = self.nodes.len() as u32;
        let level = self.sample_level(vid.0, self.nodes.len());
        let mut layers = Vec::with_capacity(level + 1);
        for l in 0..=level {
            let cap = if l == 0 { self.m0 } else { self.m };
            layers.push(Vec::with_capacity(cap));
        }

        self.nodes.push(HnswNode {
            vid,
            vector: vector.clone(),
            norm,
            deleted: false,
            layers,
        });
        self.vid_to_idx.insert(vid.0, new_idx);
        self.active_count += 1;

        let ep = match self.entry_point {
            Some(ep) => ep,
            None => {
                self.entry_point = Some(new_idx);
                self.max_level = level;
                return;
            }
        };

        let mut curr_obj = ep;
        let mut curr_dist = self.distance(&vector, norm, curr_obj, "cosine");

        // Phase 1: Greedy descent from top layer down to level + 1
        if self.max_level > level {
            for l in ((level + 1)..=self.max_level).rev() {
                let mut changed = true;
                while changed {
                    changed = false;
                    let neighbors = self.nodes[curr_obj as usize]
                        .layers
                        .get(l)
                        .cloned()
                        .unwrap_or_default();
                    for nb in neighbors {
                        let d = self.distance(&vector, norm, nb, "cosine");
                        if d < curr_dist {
                            curr_dist = d;
                            curr_obj = nb;
                            changed = true;
                        }
                    }
                }
            }
        }

        // Phase 2: Beam search and bidirectional linking from min(level, max_level) down to 0
        let top_connect_level = level.min(self.max_level);
        for l in (0..=top_connect_level).rev() {
            let candidates =
                self.search_layer(&vector, norm, curr_obj, self.ef_construction, l, "cosine");
            if let Some(closest) = candidates.first() {
                curr_obj = closest.idx;
            }

            let max_m = if l == 0 { self.m0 } else { self.m };
            let selected: Vec<u32> = candidates
                .into_iter()
                .filter(|c| c.idx != new_idx)
                .take(max_m)
                .map(|c| c.idx)
                .collect();

            self.nodes[new_idx as usize].layers[l] = selected.clone();

            for nb_idx in selected {
                if l < self.nodes[nb_idx as usize].layers.len() {
                    let nb_list = &mut self.nodes[nb_idx as usize].layers[l];
                    if !nb_list.contains(&new_idx) {
                        nb_list.push(new_idx);
                    }
                    if nb_list.len() > max_m {
                        let nb_vec = self.nodes[nb_idx as usize].vector.clone();
                        let nb_norm = self.nodes[nb_idx as usize].norm;
                        let mut scored: Vec<(f32, u32)> = self.nodes[nb_idx as usize].layers[l]
                            .iter()
                            .map(|&cand| (self.distance(&nb_vec, nb_norm, cand, "cosine"), cand))
                            .collect();
                        scored.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(Ordering::Equal));
                        self.nodes[nb_idx as usize].layers[l] =
                            scored.into_iter().take(max_m).map(|(_, idx)| idx).collect();
                    }
                }
            }
        }

        if level > self.max_level {
            self.max_level = level;
            self.entry_point = Some(new_idx);
        }
    }

    /// Marks a vertex as deleted in the HNSW index.
    pub fn remove(&mut self, vid: VertexId) -> bool {
        if let Some(&idx) = self.vid_to_idx.get(&vid.0) {
            let node = &mut self.nodes[idx as usize];
            if !node.deleted {
                node.deleted = true;
                self.active_count = self.active_count.saturating_sub(1);
                return true;
            }
        }
        false
    }

    /// Performs beam search at a single layer `layer`, returning candidates sorted by ascending distance.
    fn search_layer(
        &self,
        query: &[f32],
        query_norm: f32,
        entry_point: u32,
        ef: usize,
        layer: usize,
        metric: &str,
    ) -> Vec<DistCandidate> {
        let mut visited = vec![false; self.nodes.len()];
        visited[entry_point as usize] = true;

        let init_dist = self.distance(query, query_norm, entry_point, metric);
        let init_cand = DistCandidate {
            dist: init_dist,
            idx: entry_point,
        };

        let mut candidates = BinaryHeap::with_capacity(ef * 2);
        let mut top_results = BinaryHeap::with_capacity(ef + 1);

        candidates.push(MinCandidate(init_cand));
        top_results.push(MaxCandidate(init_cand));

        while let Some(MinCandidate(curr)) = candidates.pop() {
            let worst_dist = top_results
                .peek()
                .map(|m| m.0.dist)
                .unwrap_or(f32::INFINITY);
            if curr.dist > worst_dist && top_results.len() >= ef {
                break;
            }

            if let Some(neighbors) = self.nodes[curr.idx as usize].layers.get(layer) {
                for &nb_idx in neighbors {
                    let nb_usize = nb_idx as usize;
                    if !visited[nb_usize] {
                        visited[nb_usize] = true;
                        let d = self.distance(query, query_norm, nb_idx, metric);
                        let worst = top_results
                            .peek()
                            .map(|m| m.0.dist)
                            .unwrap_or(f32::INFINITY);

                        if top_results.len() < ef || d < worst {
                            let cand = DistCandidate {
                                dist: d,
                                idx: nb_idx,
                            };
                            candidates.push(MinCandidate(cand));
                            top_results.push(MaxCandidate(cand));
                            if top_results.len() > ef {
                                top_results.pop();
                            }
                        }
                    }
                }
            }
        }

        let mut res: Vec<DistCandidate> = top_results.into_iter().map(|m| m.0).collect();
        res.sort_by(|a, b| a.dist.partial_cmp(&b.dist).unwrap_or(Ordering::Equal));
        res
    }

    /// Searches for the top-K nearest neighbors using HNSW multi-layer navigation.
    pub fn search(&self, query: &[f32], k: usize, metric: &str) -> Vec<(VertexId, f32)> {
        if k == 0 || self.active_count == 0 || query.len() != self.dim {
            return Vec::new();
        }

        let metric_norm = metric.to_lowercase();
        let query_norm = Self::compute_norm(query);

        // For small collections (or when k approaches active_count), vectorized flat scan over
        // precomputed norms is sub-millisecond and guarantees 100% exact recall.
        if self.active_count <= 512 || k * 2 >= self.active_count {
            let mut results: Vec<(VertexId, f32)> = self
                .nodes
                .iter()
                .enumerate()
                .filter(|(_, n)| !n.deleted)
                .map(|(idx, n)| {
                    (
                        n.vid,
                        self.similarity_score(query, query_norm, idx as u32, &metric_norm),
                    )
                })
                .collect();

            results.sort_by(|a, b| {
                b.1.partial_cmp(&a.1)
                    .unwrap_or(Ordering::Equal)
                    .then_with(|| a.0.cmp(&b.0))
            });
            results.truncate(k);
            return results;
        }

        let ep = match self.entry_point {
            Some(ep) => ep,
            None => return Vec::new(),
        };

        let mut curr_obj = ep;
        let mut curr_dist = self.distance(query, query_norm, curr_obj, &metric_norm);

        for l in (1..=self.max_level).rev() {
            let mut changed = true;
            while changed {
                changed = false;
                if let Some(neighbors) = self.nodes[curr_obj as usize].layers.get(l) {
                    for &nb in neighbors {
                        let d = self.distance(query, query_norm, nb, &metric_norm);
                        if d < curr_dist {
                            curr_dist = d;
                            curr_obj = nb;
                            changed = true;
                        }
                    }
                }
            }
        }

        let ef = self.ef_search.max(k * 4).max(64);
        let candidates = self.search_layer(query, query_norm, curr_obj, ef, 0, &metric_norm);

        let mut results: Vec<(VertexId, f32)> = candidates
            .into_iter()
            .filter_map(|c| {
                let node = &self.nodes[c.idx as usize];
                if node.deleted {
                    None
                } else {
                    Some((
                        node.vid,
                        self.similarity_score(query, query_norm, c.idx, &metric_norm),
                    ))
                }
            })
            .collect();

        results.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(Ordering::Equal)
                .then_with(|| a.0.cmp(&b.0))
        });
        results.truncate(k);
        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hnsw_index_ann_search_recall() {
        let dim = 8;
        let mut hnsw = HnswIndex::new(dim, 16, 64, 64);

        for i in 1..=1000u64 {
            let mut vec = vec![0.0f32; dim];
            for d in 0..dim {
                let h = i
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add((d as u64 + 1).wrapping_mul(1442695040888963407));
                vec[d] = (((h >> 32) % 1000) as f32 + 1.0) / 1000.0;
            }
            hnsw.insert(VertexId(i), vec);
        }

        assert_eq!(hnsw.len(), 1000);

        // Query exact match for vertex 42
        let mut q = vec![0.0f32; dim];
        for d in 0..dim {
            let h = 42u64
                .wrapping_mul(6364136223846793005)
                .wrapping_add((d as u64 + 1).wrapping_mul(1442695040888963407));
            q[d] = (((h >> 32) % 1000) as f32 + 1.0) / 1000.0;
        }

        let top5 = hnsw.search(&q, 5, "cosine");
        assert_eq!(top5.len(), 5);
        assert_eq!(top5[0].0, VertexId(42));
        assert!((top5[0].1 - 1.0).abs() < 1e-5);

        // Remove vertex 42 and verify it's no longer returned
        assert!(hnsw.remove(VertexId(42)));
        assert_eq!(hnsw.len(), 999);
        let top5_after = hnsw.search(&q, 5, "cosine");
        assert!(top5_after.iter().all(|(vid, _)| *vid != VertexId(42)));
    }
}
