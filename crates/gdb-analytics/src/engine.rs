use crate::betweenness::betweenness_centrality;
use crate::closeness::closeness_centrality;
use crate::degree::degree_centrality;
use crate::kcore::k_core_decomposition;
use crate::louvain::louvain;
use crate::lpa::label_propagation;
use crate::node2vec::node2vec;
use crate::pagerank::pagerank;
use crate::scc::strongly_connected_components_vec;
use crate::similarity::{common_neighbors, cosine_similarity, jaccard_similarity};
use crate::sssp::single_source_shortest_path;
use crate::triangles::triangle_count;
use crate::wcc::weakly_connected_components;
use arrow::array::{ArrayRef, Float64Array, RecordBatch, UInt32Array, UInt64Array};
use arrow::datatypes::{DataType, Field, Schema};
use gdb_core::{GdbResult, VertexId};
use gdb_gpu::GpuDispatcher;
use gdb_storage::ChunkedCsr;
use std::sync::Arc;

/// Enterprise Graph Analytics Engine (Nebula Enterprise Analytics suite)
/// with seamless hardware GPU acceleration via `GpuDispatcher`.
pub struct AnalyticsEngine;

impl AnalyticsEngine {
    /// Executes PageRank and returns results as an Arrow RecordBatch.
    pub fn run_pagerank(
        csr: &ChunkedCsr,
        damping: f64,
        max_iter: usize,
        tolerance: f64,
    ) -> GdbResult<RecordBatch> {
        Self::run_pagerank_with_gpu(csr, damping, max_iter, tolerance, None)
    }

    pub fn run_pagerank_with_gpu(
        csr: &ChunkedCsr,
        damping: f64,
        max_iter: usize,
        tolerance: f64,
        gpu: Option<&GpuDispatcher>,
    ) -> GdbResult<RecordBatch> {
        let mut pairs: Vec<(u64, f64)> = if let Some(disp) = gpu {
            if disp.enabled {
                disp.pagerank(csr, damping as f32, max_iter)?
                    .into_iter()
                    .map(|(vid, s)| (vid.as_u64(), s as f64))
                    .collect()
            } else {
                disp.pagerank_f64(csr, damping, max_iter, tolerance)?
                    .into_iter()
                    .map(|(vid, s)| (vid.as_u64(), s))
                    .collect()
            }
        } else {
            pagerank(csr, damping, max_iter, tolerance)
                .into_iter()
                .map(|(vid, s)| (vid.as_u64(), s))
                .collect()
        };

        if !pairs.windows(2).all(|w| w[0].0 <= w[1].0) {
            pairs.sort_unstable_by_key(|(vid, _)| *vid);
        }
        let vids: Vec<u64> = pairs.iter().map(|(v, _)| *v).collect();
        let score_vals: Vec<f64> = pairs.iter().map(|(_, s)| *s).collect();

        let schema = Arc::new(Schema::new(vec![
            Field::new("vertex_id", DataType::UInt64, false),
            Field::new("score", DataType::Float64, false),
        ]));

        let columns: Vec<ArrayRef> = vec![
            Arc::new(UInt64Array::from(vids)),
            Arc::new(Float64Array::from(score_vals)),
        ];

        Ok(RecordBatch::try_new(schema, columns)?)
    }

    /// Executes Weakly Connected Components (WCC).
    pub fn run_wcc(csr: &ChunkedCsr) -> GdbResult<RecordBatch> {
        Self::run_wcc_with_gpu(csr, None)
    }

    pub fn run_wcc_with_gpu(
        csr: &ChunkedCsr,
        gpu: Option<&GpuDispatcher>,
    ) -> GdbResult<RecordBatch> {
        let mut pairs: Vec<(u64, u64)> = if let Some(disp) = gpu {
            disp.wcc(csr)?
                .into_iter()
                .map(|(vid, comp)| (vid.as_u64(), comp))
                .collect()
        } else {
            weakly_connected_components(csr)
                .into_iter()
                .map(|(vid, comp)| (vid.as_u64(), comp.as_u64()))
                .collect()
        };

        if !pairs.windows(2).all(|w| w[0].0 <= w[1].0) {
            pairs.sort_unstable_by_key(|(vid, _)| *vid);
        }
        let vids: Vec<u64> = pairs.iter().map(|(v, _)| *v).collect();
        let comp_ids: Vec<u64> = pairs.iter().map(|(_, c)| *c).collect();

        let schema = Arc::new(Schema::new(vec![
            Field::new("vertex_id", DataType::UInt64, false),
            Field::new("component_id", DataType::UInt64, false),
        ]));

        let columns: Vec<ArrayRef> = vec![
            Arc::new(UInt64Array::from(vids)),
            Arc::new(UInt64Array::from(comp_ids)),
        ];

        Ok(RecordBatch::try_new(schema, columns)?)
    }

    /// Executes Strongly Connected Components (SCC).
    pub fn run_scc(csr: &ChunkedCsr) -> GdbResult<RecordBatch> {
        let pairs = strongly_connected_components_vec(csr);

        let vids: Vec<u64> = pairs.iter().map(|(v, _)| *v).collect();
        let comp_ids: Vec<u64> = pairs.iter().map(|(_, c)| *c).collect();

        let schema = Arc::new(Schema::new(vec![
            Field::new("vertex_id", DataType::UInt64, false),
            Field::new("component_id", DataType::UInt64, false),
        ]));

        let columns: Vec<ArrayRef> = vec![
            Arc::new(UInt64Array::from(vids)),
            Arc::new(UInt64Array::from(comp_ids)),
        ];

        Ok(RecordBatch::try_new(schema, columns)?)
    }

    /// Executes Louvain Community Detection.
    pub fn run_louvain(csr: &ChunkedCsr, max_iter: usize) -> GdbResult<RecordBatch> {
        Self::run_louvain_with_gpu(csr, max_iter, None)
    }

    pub fn run_louvain_with_gpu(
        csr: &ChunkedCsr,
        max_iter: usize,
        gpu: Option<&GpuDispatcher>,
    ) -> GdbResult<RecordBatch> {
        let mut pairs: Vec<(u64, u64)> = if let Some(disp) = gpu {
            disp.louvain(csr, max_iter)?
                .into_iter()
                .map(|(vid, c)| (vid.as_u64(), c))
                .collect()
        } else {
            louvain(csr, max_iter)
                .into_iter()
                .map(|(vid, c)| (vid.as_u64(), c))
                .collect()
        };

        if !pairs.windows(2).all(|w| w[0].0 <= w[1].0) {
            pairs.sort_unstable_by_key(|(vid, _)| *vid);
        }
        let vids: Vec<u64> = pairs.iter().map(|(v, _)| *v).collect();
        let comm_ids: Vec<u64> = pairs.iter().map(|(_, c)| *c).collect();

        let schema = Arc::new(Schema::new(vec![
            Field::new("vertex_id", DataType::UInt64, false),
            Field::new("community_id", DataType::UInt64, false),
        ]));

        let columns: Vec<ArrayRef> = vec![
            Arc::new(UInt64Array::from(vids)),
            Arc::new(UInt64Array::from(comm_ids)),
        ];

        Ok(RecordBatch::try_new(schema, columns)?)
    }

    /// Executes Label Propagation Algorithm (LPA).
    pub fn run_lpa(csr: &ChunkedCsr, max_iter: usize) -> GdbResult<RecordBatch> {
        Self::run_lpa_with_gpu(csr, max_iter, None)
    }

    pub fn run_lpa_with_gpu(
        csr: &ChunkedCsr,
        max_iter: usize,
        gpu: Option<&GpuDispatcher>,
    ) -> GdbResult<RecordBatch> {
        let mut pairs: Vec<(u64, u64)> = if let Some(disp) = gpu {
            disp.lpa(csr, max_iter)?
                .into_iter()
                .map(|(vid, l)| (vid.as_u64(), l))
                .collect()
        } else {
            label_propagation(csr, max_iter)
                .into_iter()
                .map(|(vid, l)| (vid.as_u64(), l))
                .collect()
        };

        if !pairs.windows(2).all(|w| w[0].0 <= w[1].0) {
            pairs.sort_unstable_by_key(|(vid, _)| *vid);
        }
        let vids: Vec<u64> = pairs.iter().map(|(v, _)| *v).collect();
        let label_ids: Vec<u64> = pairs.iter().map(|(_, l)| *l).collect();

        let schema = Arc::new(Schema::new(vec![
            Field::new("vertex_id", DataType::UInt64, false),
            Field::new("community_id", DataType::UInt64, false),
        ]));

        let columns: Vec<ArrayRef> = vec![
            Arc::new(UInt64Array::from(vids)),
            Arc::new(UInt64Array::from(label_ids)),
        ];

        Ok(RecordBatch::try_new(schema, columns)?)
    }

    /// Executes K-Core Decomposition.
    pub fn run_kcore(csr: &ChunkedCsr) -> GdbResult<RecordBatch> {
        Self::run_kcore_with_gpu(csr, None)
    }

    pub fn run_kcore_with_gpu(
        csr: &ChunkedCsr,
        gpu: Option<&GpuDispatcher>,
    ) -> GdbResult<RecordBatch> {
        let mut pairs: Vec<(u64, u32)> = if let Some(disp) = gpu {
            disp.kcore(csr)?
                .into_iter()
                .map(|(vid, k)| (vid.as_u64(), k))
                .collect()
        } else {
            k_core_decomposition(csr)
                .into_iter()
                .map(|(vid, k)| (vid.as_u64(), k))
                .collect()
        };

        if !pairs.windows(2).all(|w| w[0].0 <= w[1].0) {
            pairs.sort_unstable_by_key(|(vid, _)| *vid);
        }
        let vids: Vec<u64> = pairs.iter().map(|(v, _)| *v).collect();
        let core_nums: Vec<u32> = pairs.iter().map(|(_, k)| *k).collect();

        let schema = Arc::new(Schema::new(vec![
            Field::new("vertex_id", DataType::UInt64, false),
            Field::new("coreness", DataType::UInt32, false),
        ]));

        let columns: Vec<ArrayRef> = vec![
            Arc::new(UInt64Array::from(vids)),
            Arc::new(UInt32Array::from(core_nums)),
        ];

        Ok(RecordBatch::try_new(schema, columns)?)
    }

    /// Executes Triangle Count & Clustering Coefficient.
    pub fn run_triangle_count(csr: &ChunkedCsr) -> GdbResult<RecordBatch> {
        Self::run_triangle_count_with_gpu(csr, None)
    }

    pub fn run_triangle_count_with_gpu(
        csr: &ChunkedCsr,
        gpu: Option<&GpuDispatcher>,
    ) -> GdbResult<RecordBatch> {
        let mut rows: Vec<(u64, u64, f64)> = if let Some(disp) = gpu {
            disp.triangle_count_with_lcc(csr)?
                .into_iter()
                .map(|(vid, t, lcc)| (vid.as_u64(), t, lcc))
                .collect()
        } else {
            triangle_count(csr)
                .into_iter()
                .map(|(vid, m)| (vid.as_u64(), m.triangles, m.clustering_coefficient))
                .collect()
        };

        if !rows.windows(2).all(|w| w[0].0 <= w[1].0) {
            rows.sort_unstable_by_key(|(vid, _, _)| *vid);
        }
        let vids: Vec<u64> = rows.iter().map(|(v, _, _)| *v).collect();
        let triangles: Vec<u64> = rows.iter().map(|(_, t, _)| *t).collect();
        let coeffs: Vec<f64> = rows.iter().map(|(_, _, c)| *c).collect();

        let schema = Arc::new(Schema::new(vec![
            Field::new("vertex_id", DataType::UInt64, false),
            Field::new("triangles", DataType::UInt64, false),
            Field::new("clustering_coefficient", DataType::Float64, false),
        ]));

        let columns: Vec<ArrayRef> = vec![
            Arc::new(UInt64Array::from(vids)),
            Arc::new(UInt64Array::from(triangles)),
            Arc::new(Float64Array::from(coeffs)),
        ];

        Ok(RecordBatch::try_new(schema, columns)?)
    }

    /// Executes Betweenness Centrality.
    pub fn run_betweenness(csr: &ChunkedCsr, normalized: bool) -> GdbResult<RecordBatch> {
        Self::run_betweenness_with_gpu(csr, normalized, None)
    }

    pub fn run_betweenness_with_gpu(
        csr: &ChunkedCsr,
        normalized: bool,
        gpu: Option<&GpuDispatcher>,
    ) -> GdbResult<RecordBatch> {
        let mut pairs: Vec<(u64, f64)> = if let Some(disp) = gpu {
            disp.betweenness(csr, normalized)?
                .into_iter()
                .map(|(vid, b)| (vid.as_u64(), b))
                .collect()
        } else {
            betweenness_centrality(csr, normalized)
                .into_iter()
                .map(|(vid, b)| (vid.as_u64(), b))
                .collect()
        };

        if !pairs.windows(2).all(|w| w[0].0 <= w[1].0) {
            pairs.sort_unstable_by_key(|(vid, _)| *vid);
        }
        let vids: Vec<u64> = pairs.iter().map(|(v, _)| *v).collect();
        let scores: Vec<f64> = pairs.iter().map(|(_, b)| *b).collect();

        let schema = Arc::new(Schema::new(vec![
            Field::new("vertex_id", DataType::UInt64, false),
            Field::new("betweenness", DataType::Float64, false),
        ]));

        let columns: Vec<ArrayRef> = vec![
            Arc::new(UInt64Array::from(vids)),
            Arc::new(Float64Array::from(scores)),
        ];

        Ok(RecordBatch::try_new(schema, columns)?)
    }

    /// Executes Closeness Centrality.
    pub fn run_closeness(csr: &ChunkedCsr) -> GdbResult<RecordBatch> {
        Self::run_closeness_with_gpu(csr, None)
    }

    pub fn run_closeness_with_gpu(
        csr: &ChunkedCsr,
        gpu: Option<&GpuDispatcher>,
    ) -> GdbResult<RecordBatch> {
        let mut pairs: Vec<(u64, f64)> = if let Some(disp) = gpu {
            disp.closeness(csr)?
                .into_iter()
                .map(|(vid, c)| (vid.as_u64(), c))
                .collect()
        } else {
            closeness_centrality(csr)
                .into_iter()
                .map(|(vid, c)| (vid.as_u64(), c))
                .collect()
        };

        if !pairs.windows(2).all(|w| w[0].0 <= w[1].0) {
            pairs.sort_unstable_by_key(|(vid, _)| *vid);
        }
        let vids: Vec<u64> = pairs.iter().map(|(v, _)| *v).collect();
        let scores: Vec<f64> = pairs.iter().map(|(_, c)| *c).collect();

        let schema = Arc::new(Schema::new(vec![
            Field::new("vertex_id", DataType::UInt64, false),
            Field::new("closeness", DataType::Float64, false),
        ]));

        let columns: Vec<ArrayRef> = vec![
            Arc::new(UInt64Array::from(vids)),
            Arc::new(Float64Array::from(scores)),
        ];

        Ok(RecordBatch::try_new(schema, columns)?)
    }

    /// Executes Degree Centrality.
    pub fn run_degree(csr: &ChunkedCsr) -> GdbResult<RecordBatch> {
        Self::run_degree_with_gpu(csr, None)
    }

    pub fn run_degree_with_gpu(
        csr: &ChunkedCsr,
        gpu: Option<&GpuDispatcher>,
    ) -> GdbResult<RecordBatch> {
        let mut rows: Vec<(u64, u32, u32, u32)> = if let Some(disp) = gpu {
            disp.degree(csr)?
                .into_iter()
                .map(|(vid, i, o, t)| (vid.as_u64(), i, o, t))
                .collect()
        } else {
            degree_centrality(csr)
                .into_iter()
                .map(|(vid, m)| (vid.as_u64(), m.in_degree, m.out_degree, m.total_degree))
                .collect()
        };

        if !rows.windows(2).all(|w| w[0].0 <= w[1].0) {
            rows.sort_unstable_by_key(|(vid, _, _, _)| *vid);
        }
        let vids: Vec<u64> = rows.iter().map(|(v, _, _, _)| *v).collect();
        let in_degs: Vec<u32> = rows.iter().map(|(_, i, _, _)| *i).collect();
        let out_degs: Vec<u32> = rows.iter().map(|(_, _, o, _)| *o).collect();
        let tot_degs: Vec<u32> = rows.iter().map(|(_, _, _, t)| *t).collect();

        let schema = Arc::new(Schema::new(vec![
            Field::new("vertex_id", DataType::UInt64, false),
            Field::new("in_degree", DataType::UInt32, false),
            Field::new("out_degree", DataType::UInt32, false),
            Field::new("total_degree", DataType::UInt32, false),
        ]));

        let columns: Vec<ArrayRef> = vec![
            Arc::new(UInt64Array::from(vids)),
            Arc::new(UInt32Array::from(in_degs)),
            Arc::new(UInt32Array::from(out_degs)),
            Arc::new(UInt32Array::from(tot_degs)),
        ];

        Ok(RecordBatch::try_new(schema, columns)?)
    }

    /// Executes Single-Source Shortest Path (SSSP).
    pub fn run_sssp(csr: &ChunkedCsr, source: VertexId) -> GdbResult<RecordBatch> {
        Self::run_sssp_with_gpu(csr, source, None)
    }

    pub fn run_sssp_with_gpu(
        csr: &ChunkedCsr,
        source: VertexId,
        gpu: Option<&GpuDispatcher>,
    ) -> GdbResult<RecordBatch> {
        let mut pairs: Vec<(u64, u32)> = if let Some(disp) = gpu {
            disp.sssp(csr, source)?
                .into_iter()
                .map(|(vid, d)| (vid.as_u64(), d))
                .collect()
        } else {
            single_source_shortest_path(csr, source)
                .into_iter()
                .map(|(vid, d)| (vid.as_u64(), d))
                .collect()
        };

        if !pairs.windows(2).all(|w| w[0].0 <= w[1].0) {
            pairs.sort_unstable_by_key(|(vid, _)| *vid);
        }
        let vids: Vec<u64> = pairs.iter().map(|(v, _)| *v).collect();
        let distances: Vec<u32> = pairs.iter().map(|(_, d)| *d).collect();

        let schema = Arc::new(Schema::new(vec![
            Field::new("vertex_id", DataType::UInt64, false),
            Field::new("distance", DataType::UInt32, false),
        ]));

        let columns: Vec<ArrayRef> = vec![
            Arc::new(UInt64Array::from(vids)),
            Arc::new(UInt32Array::from(distances)),
        ];

        Ok(RecordBatch::try_new(schema, columns)?)
    }

    /// Executes Similarity metrics between two nodes.
    pub fn run_similarity(
        csr: &ChunkedCsr,
        node1: VertexId,
        node2: VertexId,
    ) -> GdbResult<RecordBatch> {
        let jaccard = jaccard_similarity(csr, node1, node2);
        let cosine = cosine_similarity(csr, node1, node2);
        let common = common_neighbors(csr, node1, node2) as u64;

        let schema = Arc::new(Schema::new(vec![
            Field::new("node1", DataType::UInt64, false),
            Field::new("node2", DataType::UInt64, false),
            Field::new("jaccard", DataType::Float64, false),
            Field::new("cosine", DataType::Float64, false),
            Field::new("common_neighbors", DataType::UInt64, false),
        ]));

        let columns: Vec<ArrayRef> = vec![
            Arc::new(UInt64Array::from(vec![node1.as_u64()])),
            Arc::new(UInt64Array::from(vec![node2.as_u64()])),
            Arc::new(Float64Array::from(vec![jaccard])),
            Arc::new(Float64Array::from(vec![cosine])),
            Arc::new(UInt64Array::from(vec![common])),
        ];

        Ok(RecordBatch::try_new(schema, columns)?)
    }

    /// Executes Node2Vec representation learning on CSR and returns (vertex_id, embedding).
    pub fn run_node2vec(
        csr: &ChunkedCsr,
        dimensions: usize,
        walk_length: usize,
        num_walks: usize,
        p: f64,
        q: f64,
    ) -> GdbResult<RecordBatch> {
        let results = node2vec(csr, dimensions, walk_length, num_walks, p, q);

        let mut vids = Vec::with_capacity(results.len());
        let mut str_builder =
            arrow::array::StringBuilder::with_capacity(results.len(), results.len() * (dimensions * 8 + 4));
        let mut buf = String::with_capacity(dimensions * 10 + 4);

        for (vid, emb) in results {
            vids.push(vid.as_u64());
            buf.clear();
            buf.push('[');
            for (idx, &x) in emb.iter().enumerate() {
                if idx > 0 {
                    buf.push_str(", ");
                }
                let _ = std::fmt::Write::write_fmt(&mut buf, format_args!("{:.4}", x));
            }
            buf.push(']');
            str_builder.append_value(&buf);
        }

        let schema = Arc::new(Schema::new(vec![
            Field::new("vertex_id", DataType::UInt64, false),
            Field::new("embedding", DataType::Utf8, false),
        ]));

        let columns: Vec<ArrayRef> = vec![
            Arc::new(UInt64Array::from(vids)),
            Arc::new(str_builder.finish()),
        ];

        Ok(RecordBatch::try_new(schema, columns)?)
    }
}
