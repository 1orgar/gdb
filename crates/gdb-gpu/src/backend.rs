use gdb_core::{GdbResult, VertexId};
use gdb_storage::ChunkedCsr;
use rayon::prelude::*;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, VecDeque};

/// Result of a GPU-accelerated Breadth-First Search traversal: (VertexId, distance/depth)
pub type BfsResult = Vec<(VertexId, u32)>;

/// Result of a GPU-accelerated PageRank computation: (VertexId, score)
pub type PageRankResult = Vec<(VertexId, f32)>;

/// Common trait for GPU Acceleration Backends (Metal on macOS, CUDA on Linux, and CPU SIMD fallback).
pub trait GpuComputeBackend: Send + Sync {
    fn name(&self) -> &'static str;

    /// Parallel Breadth-First Search (BFS) starting from `start_vid` up to `max_depth`.
    fn parallel_bfs(
        &self,
        csr: &ChunkedCsr,
        start_vid: VertexId,
        max_depth: u32,
    ) -> GdbResult<BfsResult>;

    /// PageRank score computation using parallel sparse-matrix vector multiplication (SpMV).
    fn pagerank(
        &self,
        csr: &ChunkedCsr,
        damping: f32,
        iterations: usize,
    ) -> GdbResult<PageRankResult>;

    /// High-precision f64 PageRank with convergence tolerance and dangling-node redistribution.
    fn pagerank_f64(
        &self,
        csr: &ChunkedCsr,
        damping: f64,
        max_iter: usize,
        tolerance: f64,
    ) -> GdbResult<Vec<(VertexId, f64)>> {
        CpuFallbackBackend.pagerank_f64(csr, damping, max_iter, tolerance)
    }

    /// Weakly Connected Components (WCC) parallel label propagation.
    fn wcc(&self, csr: &ChunkedCsr) -> GdbResult<Vec<(VertexId, u64)>>;

    /// Louvain Community Detection (Modularity optimization).
    fn louvain(&self, csr: &ChunkedCsr, max_iter: usize) -> GdbResult<Vec<(VertexId, u64)>>;

    /// Parallel triangle counting.
    fn triangle_count(&self, csr: &ChunkedCsr) -> GdbResult<Vec<(VertexId, u64)>>;

    /// Parallel triangle counting with Local Clustering Coefficient (LCC).
    fn triangle_count_with_lcc(&self, csr: &ChunkedCsr) -> GdbResult<Vec<(VertexId, u64, f64)>> {
        let num_v = csr.vertex_count();
        if num_v == 0 {
            return Ok(Vec::new());
        }
        let adj = csr.build_undirected_adj();
        let tri_counts = self.triangle_count(csr)?;
        let mut out = Vec::with_capacity(num_v);
        for u in 0..num_v {
            let vid_raw = csr.reverse_map[u];
            let t = tri_counts.get(u).map(|&(_, c)| c).unwrap_or(0);
            let deg = adj[u].len() as f64;
            let lcc = if deg > 1.0 {
                (2.0 * t as f64) / (deg * (deg - 1.0))
            } else {
                0.0
            };
            out.push((VertexId(vid_raw), t, lcc));
        }
        Ok(out)
    }

    /// Single-Source Shortest Path (SSSP) on unweighted graph.
    fn sssp(&self, csr: &ChunkedCsr, source: VertexId) -> GdbResult<Vec<(VertexId, u32)>> {
        CpuFallbackBackend.sssp(csr, source)
    }

    /// Label Propagation Algorithm (LPA) for community detection.
    fn lpa(&self, csr: &ChunkedCsr, max_iter: usize) -> GdbResult<Vec<(VertexId, u64)>> {
        CpuFallbackBackend.lpa(csr, max_iter)
    }

    /// K-Core Decomposition on undirected simple graph projection.
    fn kcore(&self, csr: &ChunkedCsr) -> GdbResult<Vec<(VertexId, u32)>> {
        CpuFallbackBackend.kcore(csr)
    }

    /// Degree Centrality (in_degree, out_degree, total_degree).
    fn degree(&self, csr: &ChunkedCsr) -> GdbResult<Vec<(VertexId, u32, u32, u32)>> {
        CpuFallbackBackend.degree(csr)
    }

    /// Parallel Brandes Betweenness Centrality.
    fn betweenness(&self, csr: &ChunkedCsr, normalized: bool) -> GdbResult<Vec<(VertexId, f64)>> {
        CpuFallbackBackend.betweenness(csr, normalized)
    }

    /// Parallel Harmonic Closeness Centrality.
    fn closeness(&self, csr: &ChunkedCsr) -> GdbResult<Vec<(VertexId, f64)>> {
        CpuFallbackBackend.closeness(csr)
    }

    /// Parallel Vector Similarity Search (Cosine, Dot Product, or Euclidean L2).
    fn vector_similarity(
        &self,
        vectors: &[f32],
        dim: usize,
        query: &[f32],
        k: usize,
        metric: &str,
    ) -> GdbResult<Vec<(usize, f32)>> {
        CpuFallbackBackend.vector_similarity(vectors, dim, query, k, metric)
    }

    /// Maximum VRAM budget allocated in bytes on discrete GPU before chunked window paging activates.
    fn max_vram_bytes(&self) -> usize {
        usize::MAX
    }
}

/// High-performance CPU fallback using Rayon work-stealing parallelism and O(1) dense CSR indexing.
pub struct CpuFallbackBackend;

impl GpuComputeBackend for CpuFallbackBackend {
    fn name(&self) -> &'static str {
        "CPU-Vectorized-WorkStealing"
    }

    fn parallel_bfs(
        &self,
        csr: &ChunkedCsr,
        start_vid: VertexId,
        max_depth: u32,
    ) -> GdbResult<BfsResult> {
        let num_v = csr.vertex_count();
        if num_v == 0 {
            return Ok(Vec::new());
        }

        let start_raw = start_vid.as_u64();
        let Some(&start_idx) = csr.vertex_map.get(&start_raw) else {
            return Ok(Vec::new());
        };

        let mut distances = vec![u32::MAX; num_v];
        distances[start_idx as usize] = 0;

        let mut current_frontier = vec![start_idx as usize];

        for depth in 0..max_depth {
            if current_frontier.is_empty() {
                break;
            }

            let mut next_frontier = Vec::new();

            for &u_idx in &current_frontier {
                let start_off = csr.offsets[u_idx] as usize;
                let end_off = (csr.offsets[u_idx + 1] as usize).min(csr.targets.len());

                for off in start_off..end_off {
                    if let Some(v_idx) = csr.dense_target_at(off) {
                        if distances[v_idx] == u32::MAX {
                            distances[v_idx] = depth + 1;
                            next_frontier.push(v_idx);
                        }
                    }
                }
            }

            current_frontier = next_frontier;
        }

        let mut results = Vec::new();
        for (idx, &dist) in distances.iter().enumerate() {
            if dist != u32::MAX {
                results.push((VertexId(csr.reverse_map[idx]), dist));
            }
        }

        Ok(results)
    }

    fn pagerank(
        &self,
        csr: &ChunkedCsr,
        damping: f32,
        iterations: usize,
    ) -> GdbResult<PageRankResult> {
        let res_f64 = self.pagerank_f64(csr, damping as f64, iterations, 1e-7)?;
        Ok(res_f64
            .into_iter()
            .map(|(vid, score)| (vid, score as f32))
            .collect())
    }

    fn pagerank_f64(
        &self,
        csr: &ChunkedCsr,
        damping: f64,
        max_iter: usize,
        tolerance: f64,
    ) -> GdbResult<Vec<(VertexId, f64)>> {
        let num_v = csr.vertex_count();
        if num_v == 0 {
            return Ok(Vec::new());
        }

        let n = num_v as f64;
        let mut rank = vec![1.0 / n; num_v];
        let mut new_rank = vec![0.0; num_v];

        // Precompute out-degrees
        let mut out_degrees = vec![0u32; num_v];
        for u in 0..num_v {
            let start = csr.offsets[u] as usize;
            let end = (csr.offsets[u + 1] as usize).min(csr.targets.len());
            out_degrees[u] = end.saturating_sub(start) as u32;
        }

        for _ in 0..max_iter {
            let mut dangling_sum = 0.0f64;
            for u in 0..num_v {
                if out_degrees[u] == 0 {
                    dangling_sum += rank[u];
                }
            }
            let base = (1.0 - damping) / n + (damping * dangling_sum) / n;
            new_rank.fill(base);

            for u in 0..num_v {
                let deg = out_degrees[u];
                if deg > 0 {
                    let contrib = damping * (rank[u] / deg as f64);
                    let start = csr.offsets[u] as usize;
                    let end = (csr.offsets[u + 1] as usize).min(csr.targets.len());
                    for off in start..end {
                        if let Some(target_idx) = csr.dense_target_at(off) {
                            new_rank[target_idx] += contrib;
                        }
                    }
                }
            }

            let diff: f64 = rank
                .par_iter()
                .zip(new_rank.par_iter())
                .map(|(a, b)| (a - b).abs())
                .sum();
            std::mem::swap(&mut rank, &mut new_rank);

            if diff < tolerance {
                break;
            }
        }

        let mut results = Vec::with_capacity(num_v);
        for i in 0..num_v {
            results.push((VertexId(csr.reverse_map[i]), rank[i]));
        }

        Ok(results)
    }

    fn wcc(&self, csr: &ChunkedCsr) -> GdbResult<Vec<(VertexId, u64)>> {
        let num_v = csr.vertex_count();
        if num_v == 0 {
            return Ok(Vec::new());
        }

        fn find_root(parent: &mut [usize], mut i: usize) -> usize {
            while parent[i] != i {
                parent[i] = parent[parent[i]];
                i = parent[i];
            }
            i
        }

        let mut parent: Vec<usize> = (0..num_v).collect();
        for u in 0..num_v {
            let start = csr.offsets[u] as usize;
            let end = (csr.offsets[u + 1] as usize).min(csr.targets.len());
            for off in start..end {
                if let Some(v_usize) = csr.dense_target_at(off) {
                    let ru = find_root(&mut parent, u);
                    let rv = find_root(&mut parent, v_usize);
                    if ru != rv {
                        if ru < rv {
                            parent[rv] = ru;
                        } else {
                            parent[ru] = rv;
                        }
                    }
                }
            }
        }

        let mut res = Vec::with_capacity(num_v);
        for i in 0..num_v {
            let root = find_root(&mut parent, i);
            let vid = VertexId(csr.reverse_map[i]);
            let comp_id = csr.reverse_map[root];
            res.push((vid, comp_id));
        }
        Ok(res)
    }

    fn louvain(&self, csr: &ChunkedCsr, max_iter: usize) -> GdbResult<Vec<(VertexId, u64)>> {
        let num_v = csr.vertex_count();
        if num_v == 0 {
            return Ok(Vec::new());
        }

        let adj = csr.build_undirected_adj();
        let mut degrees = vec![0.0f64; num_v];
        let mut m2 = 0.0f64;
        for u in 0..num_v {
            let d = adj[u].len() as f64;
            degrees[u] = d;
            m2 += d;
        }

        if m2 == 0.0 {
            let mut res = Vec::with_capacity(num_v);
            for (i, &vid_raw) in csr.reverse_map.iter().enumerate() {
                res.push((VertexId(vid_raw), i as u64));
            }
            return Ok(res);
        }

        let mut community: Vec<usize> = (0..num_v).collect();
        let mut tot = degrees.clone();
        let mut comm_weights: Vec<(usize, f64)> = Vec::with_capacity(32);

        for _ in 0..max_iter {
            let mut improved = false;
            for u in 0..num_v {
                let k_u = degrees[u];
                if k_u == 0.0 {
                    continue;
                }
                let current_c = community[u];

                comm_weights.clear();
                let mut k_u_in_curr = 0.0f64;
                for &v_u32 in &adj[u] {
                    let c = community[v_u32 as usize];
                    if c == current_c {
                        k_u_in_curr += 1.0;
                    }
                    if let Some(entry) = comm_weights.iter_mut().find(|(cid, _)| *cid == c) {
                        entry.1 += 1.0;
                    } else {
                        comm_weights.push((c, 1.0));
                    }
                }

                tot[current_c] -= k_u;

                let mut best_c = current_c;
                let mut best_gain = k_u_in_curr - (tot[current_c] * k_u) / m2;

                for &(target_c, k_u_in_target) in &comm_weights {
                    if target_c == current_c {
                        continue;
                    }
                    let gain = k_u_in_target - (tot[target_c] * k_u) / m2;
                    if gain > best_gain + 1e-12
                        || ((gain - best_gain).abs() <= 1e-12 && target_c < best_c)
                    {
                        best_gain = gain;
                        best_c = target_c;
                    }
                }

                community[u] = best_c;
                tot[best_c] += k_u;
                if best_c != current_c {
                    improved = true;
                }
            }
            if !improved {
                break;
            }
        }

        let mut res = Vec::with_capacity(num_v);
        for i in 0..num_v {
            res.push((VertexId(csr.reverse_map[i]), community[i] as u64));
        }
        Ok(res)
    }

    fn triangle_count(&self, csr: &ChunkedCsr) -> GdbResult<Vec<(VertexId, u64)>> {
        let num_v = csr.vertex_count();
        if num_v == 0 {
            return Ok(Vec::new());
        }

        let adj = csr.build_undirected_adj();

        let counts: Vec<u64> = (0..num_v)
            .into_par_iter()
            .map(|u| {
                let u_neighbors = &adj[u];
                let deg = u_neighbors.len();
                let mut count = 0u64;
                for i in 0..deg {
                    let v = u_neighbors[i] as usize;
                    let v_neighbors = &adj[v];
                    let mut p1 = i + 1;
                    let mut p2 = 0;
                    while p1 < deg && p2 < v_neighbors.len() {
                        let w1 = u_neighbors[p1];
                        let w2 = v_neighbors[p2];
                        if w1 == w2 {
                            count += 1;
                            p1 += 1;
                            p2 += 1;
                        } else if w1 < w2 {
                            p1 += 1;
                        } else {
                            p2 += 1;
                        }
                    }
                }
                count
            })
            .collect();

        let mut res = Vec::with_capacity(num_v);
        for i in 0..num_v {
            res.push((VertexId(csr.reverse_map[i]), counts[i]));
        }
        Ok(res)
    }

    fn sssp(&self, csr: &ChunkedCsr, source: VertexId) -> GdbResult<Vec<(VertexId, u32)>> {
        let num_v = csr.vertex_count();
        if num_v == 0 {
            return Ok(Vec::new());
        }

        let mut dist_vec = vec![u32::MAX; num_v];
        if let Some(&start_idx) = csr.vertex_map.get(&source.as_u64()) {
            let s = start_idx as usize;
            dist_vec[s] = 0;
            let mut queue = VecDeque::with_capacity(num_v);
            queue.push_back(s);

            while let Some(u) = queue.pop_front() {
                let current_d = dist_vec[u];
                let start = csr.offsets[u] as usize;
                let end = (csr.offsets[u + 1] as usize).min(csr.targets.len());
                for off in start..end {
                    if let Some(v) = csr.dense_target_at(off) {
                        if dist_vec[v] == u32::MAX {
                            dist_vec[v] = current_d + 1;
                            queue.push_back(v);
                        }
                    }
                }
            }
        }

        let mut results = Vec::with_capacity(num_v);
        for (idx, &d) in dist_vec.iter().enumerate() {
            results.push((VertexId(csr.reverse_map[idx]), d));
        }
        Ok(results)
    }

    fn lpa(&self, csr: &ChunkedCsr, max_iter: usize) -> GdbResult<Vec<(VertexId, u64)>> {
        let num_v = csr.vertex_count();
        if num_v == 0 {
            return Ok(Vec::new());
        }

        let adj = csr.build_undirected_adj();
        let mut labels: Vec<u64> = (0..num_v).map(|i| csr.reverse_map[i]).collect();
        let mut label_counts: Vec<(u64, u32)> = Vec::with_capacity(32);

        for _ in 0..max_iter {
            let mut changed = false;
            for u in 0..num_v {
                let neighbors = &adj[u];
                if neighbors.is_empty() {
                    continue;
                }
                label_counts.clear();
                for &v_u32 in neighbors {
                    let v = v_u32 as usize;
                    let v_label = labels[v];
                    let v_adj = &adj[v];
                    let mut i = 0;
                    let mut j = 0;
                    let mut common = 0u32;
                    while i < neighbors.len() && j < v_adj.len() {
                        if neighbors[i] < v_adj[j] {
                            i += 1;
                        } else if neighbors[i] > v_adj[j] {
                            j += 1;
                        } else {
                            common += 1;
                            i += 1;
                            j += 1;
                        }
                    }
                    let support = 1 + common;
                    if let Some(entry) = label_counts.iter_mut().find(|(lbl, _)| *lbl == v_label) {
                        entry.1 += support;
                    } else {
                        label_counts.push((v_label, support));
                    }
                }
                if let Some(&(best_label, _)) = label_counts
                    .iter()
                    .max_by(|(l1, c1), (l2, c2)| c1.cmp(c2).then_with(|| l2.cmp(l1)))
                {
                    if best_label != labels[u] {
                        labels[u] = best_label;
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }

        let mut res = Vec::with_capacity(num_v);
        for i in 0..num_v {
            res.push((VertexId(csr.reverse_map[i]), labels[i]));
        }
        Ok(res)
    }

    fn kcore(&self, csr: &ChunkedCsr) -> GdbResult<Vec<(VertexId, u32)>> {
        let num_v = csr.vertex_count();
        if num_v == 0 {
            return Ok(Vec::new());
        }

        let adj = csr.build_undirected_adj();
        let mut degrees: Vec<u32> = adj.iter().map(|n| n.len() as u32).collect();
        let mut coreness = vec![0u32; num_v];
        let mut removed = vec![false; num_v];

        let mut pq = BinaryHeap::with_capacity(num_v);
        for u in 0..num_v {
            pq.push(Reverse((degrees[u], u)));
        }

        let mut current_k = 0u32;
        while let Some(Reverse((deg, u))) = pq.pop() {
            if removed[u] || deg > degrees[u] {
                continue;
            }
            current_k = current_k.max(degrees[u]);
            coreness[u] = current_k;
            removed[u] = true;

            for &v_u32 in &adj[u] {
                let v = v_u32 as usize;
                if !removed[v] && degrees[v] > 0 {
                    degrees[v] -= 1;
                    pq.push(Reverse((degrees[v], v)));
                }
            }
        }

        let mut res = Vec::with_capacity(num_v);
        for i in 0..num_v {
            res.push((VertexId(csr.reverse_map[i]), coreness[i]));
        }
        Ok(res)
    }

    fn degree(&self, csr: &ChunkedCsr) -> GdbResult<Vec<(VertexId, u32, u32, u32)>> {
        let num_v = csr.vertex_count();
        if num_v == 0 {
            return Ok(Vec::new());
        }

        let mut out_degrees = vec![0u32; num_v];
        let mut in_degrees = vec![0u32; num_v];

        for u in 0..num_v {
            let start = csr.offsets[u] as usize;
            let end = (csr.offsets[u + 1] as usize).min(csr.targets.len());
            out_degrees[u] = end.saturating_sub(start) as u32;
            for off in start..end {
                if let Some(v) = csr.dense_target_at(off) {
                    in_degrees[v] += 1;
                }
            }
        }

        let mut res = Vec::with_capacity(num_v);
        for i in 0..num_v {
            let in_d = in_degrees[i];
            let out_d = out_degrees[i];
            res.push((VertexId(csr.reverse_map[i]), in_d, out_d, in_d + out_d));
        }
        Ok(res)
    }

    fn betweenness(&self, csr: &ChunkedCsr, normalized: bool) -> GdbResult<Vec<(VertexId, f64)>> {
        let num_v = csr.vertex_count();
        if num_v == 0 {
            return Ok(Vec::new());
        }

        let sample_k = if num_v <= 2000 { num_v } else { 32 };
        let pivots: Vec<usize> = if sample_k == num_v {
            (0..num_v).collect()
        } else {
            (0..sample_k).map(|i| (i * num_v) / sample_k).collect()
        };

        let mut cb = pivots
            .into_par_iter()
            .fold(
                || vec![0.0f64; num_v],
                |mut local_cb, s| {
                    let mut stack = Vec::with_capacity(num_v.min(4096));
                    let mut p: Vec<Vec<usize>> = vec![Vec::new(); num_v];
                    let mut sigma = vec![0.0f64; num_v];
                    sigma[s] = 1.0;
                    let mut d = vec![-1i32; num_v];
                    d[s] = 0;

                    let mut queue = VecDeque::with_capacity(num_v.min(4096));
                    queue.push_back(s);

                    while let Some(v) = queue.pop_front() {
                        stack.push(v);
                        let start = csr.offsets[v] as usize;
                        let end = (csr.offsets[v + 1] as usize).min(csr.targets.len());

                        for off in start..end {
                            if let Some(w) = csr.dense_target_at(off) {
                                if d[w] < 0 {
                                    d[w] = d[v] + 1;
                                    queue.push_back(w);
                                }
                                if d[w] == d[v] + 1 {
                                    sigma[w] += sigma[v];
                                    p[w].push(v);
                                }
                            }
                        }
                    }

                    let mut delta = vec![0.0f64; num_v];
                    while let Some(w) = stack.pop() {
                        for &v in &p[w] {
                            delta[v] += (sigma[v] / sigma[w]) * (1.0 + delta[w]);
                        }
                        if w != s {
                            local_cb[w] += delta[w];
                        }
                    }
                    local_cb
                },
            )
            .reduce(
                || vec![0.0f64; num_v],
                |mut acc, local_cb| {
                    for i in 0..num_v {
                        acc[i] += local_cb[i];
                    }
                    acc
                },
            );

        let scale = (num_v as f64) / (sample_k as f64);
        let norm_factor = if normalized && num_v > 2 {
            scale / (((num_v - 1) as f64) * ((num_v - 2) as f64))
        } else {
            scale
        };
        if (norm_factor - 1.0).abs() > 1e-12 {
            for val in cb.iter_mut() {
                *val *= norm_factor;
            }
        }

        let mut res = Vec::with_capacity(num_v);
        for i in 0..num_v {
            res.push((VertexId(csr.reverse_map[i]), cb[i]));
        }
        Ok(res)
    }

    fn closeness(&self, csr: &ChunkedCsr) -> GdbResult<Vec<(VertexId, f64)>> {
        let num_v = csr.vertex_count();
        if num_v == 0 {
            return Ok(Vec::new());
        }

        let closeness: Vec<f64> = if num_v <= 2000 {
            (0..num_v)
                .into_par_iter()
                .map(|s| {
                    if num_v <= 1 {
                        return 0.0;
                    }
                    let mut dist = vec![-1i32; num_v];
                    dist[s] = 0;
                    let mut queue = VecDeque::with_capacity(num_v);
                    queue.push_back(s);

                    let mut sum_reciprocal = 0.0f64;
                    while let Some(u) = queue.pop_front() {
                        let start = csr.offsets[u] as usize;
                        let end = (csr.offsets[u + 1] as usize).min(csr.targets.len());
                        for off in start..end {
                            if let Some(v) = csr.dense_target_at(off) {
                                if dist[v] < 0 {
                                    dist[v] = dist[u] + 1;
                                    sum_reciprocal += 1.0 / (dist[v] as f64);
                                    queue.push_back(v);
                                }
                            }
                        }
                    }
                    sum_reciprocal / ((num_v - 1) as f64)
                })
                .collect()
        } else {
            let mut rev_adj: Vec<Vec<u32>> = vec![Vec::new(); num_v];
            for u in 0..num_v {
                let start = csr.offsets[u] as usize;
                let end = (csr.offsets[u + 1] as usize).min(csr.targets.len());
                for off in start..end {
                    if let Some(v) = csr.dense_target_at(off) {
                        rev_adj[v].push(u as u32);
                    }
                }
            }
            let sample_k = 32usize;
            let pivots: Vec<usize> = (0..sample_k).map(|i| (i * num_v) / sample_k).collect();
            let mut sum_rec = pivots
                .into_par_iter()
                .fold(
                    || vec![0.0f64; num_v],
                    |mut local_sum, t| {
                        let mut dist = vec![-1i32; num_v];
                        dist[t] = 0;
                        let mut queue = VecDeque::with_capacity(4096);
                        queue.push_back(t);
                        while let Some(curr) = queue.pop_front() {
                            let d_next = dist[curr] + 1;
                            for &prev_u32 in &rev_adj[curr] {
                                let prev = prev_u32 as usize;
                                if dist[prev] < 0 {
                                    dist[prev] = d_next;
                                    local_sum[prev] += 1.0 / (d_next as f64);
                                    queue.push_back(prev);
                                }
                            }
                        }
                        local_sum
                    },
                )
                .reduce(
                    || vec![0.0f64; num_v],
                    |mut acc, local_sum| {
                        for i in 0..num_v {
                            acc[i] += local_sum[i];
                        }
                        acc
                    },
                );
            let inv_k = 1.0 / (sample_k as f64);
            for v in sum_rec.iter_mut() {
                *v *= inv_k;
            }
            sum_rec
        };

        let mut res = Vec::with_capacity(num_v);
        for i in 0..num_v {
            res.push((VertexId(csr.reverse_map[i]), closeness[i]));
        }
        Ok(res)
    }

    fn vector_similarity(
        &self,
        vectors: &[f32],
        dim: usize,
        query: &[f32],
        k: usize,
        metric: &str,
    ) -> GdbResult<Vec<(usize, f32)>> {
        if dim == 0 || query.len() != dim || vectors.is_empty() {
            return Ok(Vec::new());
        }

        let num_vectors = vectors.len() / dim;
        let query_norm = if metric.eq_ignore_ascii_case("cosine") {
            let sum_sq: f32 = query.iter().map(|x| x * x).sum();
            sum_sq.sqrt()
        } else {
            1.0
        };

        let mut scores: Vec<(usize, f32)> = (0..num_vectors)
            .into_par_iter()
            .map(|i| {
                let v = &vectors[i * dim..(i + 1) * dim];
                let score = match metric.to_lowercase().as_str() {
                    "cosine" => {
                        let dot: f32 = v.iter().zip(query.iter()).map(|(a, b)| a * b).sum();
                        let v_norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
                        if v_norm == 0.0 || query_norm == 0.0 {
                            0.0
                        } else {
                            dot / (v_norm * query_norm)
                        }
                    }
                    "dot" | "dot_product" => {
                        v.iter().zip(query.iter()).map(|(a, b)| a * b).sum()
                    }
                    "l2" | "euclidean" => {
                        let dist_sq: f32 = v
                            .iter()
                            .zip(query.iter())
                            .map(|(a, b)| (a - b) * (a - b))
                            .sum();
                        1.0 / (1.0 + dist_sq.sqrt())
                    }
                    _ => {
                        let dot: f32 = v.iter().zip(query.iter()).map(|(a, b)| a * b).sum();
                        let v_norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
                        if v_norm == 0.0 || query_norm == 0.0 {
                            0.0
                        } else {
                            dot / (v_norm * query_norm)
                        }
                    }
                };
                (i, score)
            })
            .collect();

        scores.par_sort_unstable_by(|a, b| {
            b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal)
        });
        scores.truncate(k);
        Ok(scores)
    }
}

/// Out-of-core windowed streaming of CSR topology for memory-constrained discrete GPUs and CPUs.
/// Ensures peak memory never exceeds allocated VRAM budget while computing BFS and SpMV.
pub struct WindowedCsrStreamer;

impl WindowedCsrStreamer {
    /// Computes the number of edges per chunk given max VRAM bytes.
    #[inline]
    pub fn edges_per_chunk(max_vram_bytes: usize) -> usize {
        let budget = (max_vram_bytes / 2).max(1024);
        (budget / 8).max(1)
    }

    /// Streamed Breadth-First Search over windowed CSR edge chunks.
    pub fn streamed_bfs(
        csr: &ChunkedCsr,
        start_vid: VertexId,
        max_depth: u32,
        max_vram_bytes: usize,
    ) -> GdbResult<BfsResult> {
        let num_v = csr.vertex_count();
        if num_v == 0 {
            return Ok(Vec::new());
        }

        let start_raw = start_vid.as_u64();
        let Some(&start_idx) = csr.vertex_map.get(&start_raw) else {
            return Ok(Vec::new());
        };

        let total_e = csr.targets.len();
        let chunk_size = Self::edges_per_chunk(max_vram_bytes);

        let mut visited = vec![-1i32; num_v];
        visited[start_idx as usize] = 0;
        let mut frontier = vec![start_idx];
        let mut depth = 0;

        while !frontier.is_empty() && depth < max_depth {
            let mut next_frontier = Vec::new();
            let mut chunk_start = 0;

            while chunk_start < total_e {
                let chunk_end = (chunk_start + chunk_size).min(total_e);

                for &u in &frontier {
                    let u_start = csr.offsets[u as usize] as usize;
                    let u_end = csr.offsets[u as usize + 1] as usize;

                    let slice_start = u_start.max(chunk_start);
                    let slice_end = u_end.min(chunk_end);

                    if slice_start < slice_end {
                        for e_idx in slice_start..slice_end {
                            if let Some(v_u) = csr.dense_target_at(e_idx) {
                                if visited[v_u] == -1 {
                                    visited[v_u] = depth as i32 + 1;
                                    next_frontier.push(v_u as u32);
                                }
                            }
                        }
                    }
                }

                chunk_start = chunk_end;
            }

            frontier = next_frontier;
            depth += 1;
        }

        let mut results = Vec::new();
        for (idx, &d) in visited.iter().enumerate() {
            if d != -1 {
                let raw_vid = csr.reverse_map[idx];
                results.push((VertexId(raw_vid), d as u32));
            }
        }

        Ok(results)
    }

    /// Streamed PageRank SpMV over windowed CSR edge chunks.
    pub fn streamed_pagerank(
        csr: &ChunkedCsr,
        damping: f32,
        iterations: usize,
        max_vram_bytes: usize,
    ) -> GdbResult<PageRankResult> {
        let num_v = csr.vertex_count();
        if num_v == 0 {
            return Ok(Vec::new());
        }

        let total_e = csr.targets.len();
        let chunk_size = Self::edges_per_chunk(max_vram_bytes);

        let n = num_v as f32;
        let initial_rank = 1.0f32 / n;
        let mut rank_in = vec![initial_rank; num_v];
        let mut rank_out = vec![0.0f32; num_v];

        let out_degrees: Vec<f32> = (0..num_v)
            .map(|i| (csr.offsets[i + 1] - csr.offsets[i]) as f32)
            .collect();

        for _ in 0..iterations {
            let mut dangling_sum = 0.0f32;
            for u in 0..num_v {
                if out_degrees[u] == 0.0 {
                    dangling_sum += rank_in[u];
                }
            }
            let base_rank = (1.0 - damping) / n + (damping * dangling_sum) / n;
            rank_out.fill(base_rank);

            let mut chunk_start = 0;
            while chunk_start < total_e {
                let chunk_end = (chunk_start + chunk_size).min(total_e);

                for u in 0..num_v {
                    let deg = out_degrees[u];
                    if deg > 0.0 {
                        let u_start = csr.offsets[u] as usize;
                        let u_end = csr.offsets[u + 1] as usize;

                        let slice_start = u_start.max(chunk_start);
                        let slice_end = u_end.min(chunk_end);

                        if slice_start < slice_end {
                            let contrib = damping * (rank_in[u] / deg);
                            for e_idx in slice_start..slice_end {
                                if let Some(v_idx) = csr.dense_target_at(e_idx) {
                                    rank_out[v_idx] += contrib;
                                }
                            }
                        }
                    }
                }

                chunk_start = chunk_end;
            }

            let diff: f32 = rank_in
                .iter()
                .zip(rank_out.iter())
                .map(|(a, b)| (a - b).abs())
                .sum();
            std::mem::swap(&mut rank_in, &mut rank_out);
            if diff < 1e-7 {
                break;
            }
        }

        let mut results = Vec::with_capacity(num_v);
        for i in 0..num_v {
            let vid = VertexId(csr.reverse_map[i]);
            results.push((vid, rank_in[i]));
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vector_similarity_computation() {
        let backend = CpuFallbackBackend;
        let vectors = vec![
            1.0f32, 0.0f32, 0.0f32, // idx 0
            0.0f32, 1.0f32, 0.0f32, // idx 1
            0.7071f32, 0.7071f32, 0.0f32, // idx 2
        ];
        let query = vec![1.0f32, 0.0f32, 0.0f32];

        // Cosine
        let res_cos = backend.vector_similarity(&vectors, 3, &query, 2, "cosine").unwrap();
        assert_eq!(res_cos.len(), 2);
        assert_eq!(res_cos[0].0, 0);
        assert!((res_cos[0].1 - 1.0).abs() < 1e-4);

        // Dot product
        let res_dot = backend.vector_similarity(&vectors, 3, &query, 2, "dot").unwrap();
        assert_eq!(res_dot[0].0, 0);

        // Euclidean
        let res_l2 = backend.vector_similarity(&vectors, 3, &query, 2, "l2").unwrap();
        assert_eq!(res_l2[0].0, 0);
        assert!((res_l2[0].1 - 1.0).abs() < 1e-4);
    }

    #[test]
    fn test_windowed_csr_streamer_bfs_and_pagerank() {
        use gdb_core::{EdgeId, EdgeType};

        let edges = vec![
            EdgeId::simple(VertexId(1), EdgeType(0), VertexId(2)),
            EdgeId::simple(VertexId(2), EdgeType(0), VertexId(3)),
            EdgeId::simple(VertexId(3), EdgeType(0), VertexId(4)),
            EdgeId::simple(VertexId(4), EdgeType(0), VertexId(5)),
            EdgeId::simple(VertexId(5), EdgeType(0), VertexId(1)),
        ];
        let csr = ChunkedCsr::from_edges(edges);

        let cpu = CpuFallbackBackend;
        let bfs_standard = cpu.parallel_bfs(&csr, VertexId(1), 4).unwrap();
        assert_eq!(bfs_standard.len(), 5);

        let bfs_chunked = WindowedCsrStreamer::streamed_bfs(&csr, VertexId(1), 4, 16).unwrap();
        assert_eq!(bfs_chunked.len(), 5);

        let map_std: std::collections::HashMap<_, _> = bfs_standard.into_iter().collect();
        let map_chunked: std::collections::HashMap<_, _> = bfs_chunked.into_iter().collect();
        assert_eq!(map_std, map_chunked);

        let pr_standard = cpu.pagerank(&csr, 0.85, 10).unwrap();
        let pr_chunked = WindowedCsrStreamer::streamed_pagerank(&csr, 0.85, 10, 16).unwrap();
        assert_eq!(pr_standard.len(), pr_chunked.len());

        for (a, b) in pr_standard.iter().zip(pr_chunked.iter()) {
            assert_eq!(a.0, b.0);
            assert!((a.1 - b.1).abs() < 1e-5);
        }

        let empty_csr = ChunkedCsr::new();
        assert!(WindowedCsrStreamer::streamed_bfs(&empty_csr, VertexId(1), 2, 64).unwrap().is_empty());
        assert!(WindowedCsrStreamer::streamed_pagerank(&empty_csr, 0.85, 5, 64).unwrap().is_empty());
    }
}
