use crate::betweenness::betweenness_centrality;
use crate::closeness::closeness_centrality;
use crate::degree::degree_centrality;
use crate::kcore::k_core_decomposition;
use crate::louvain::louvain;
use crate::lpa::label_propagation;
use crate::pagerank::pagerank;
use crate::scc::strongly_connected_components;
use crate::similarity::{common_neighbors, cosine_similarity, jaccard_similarity};
use crate::sssp::single_source_shortest_path;
use crate::triangles::triangle_count;
use crate::wcc::weakly_connected_components;
use arrow::array::{ArrayRef, Float64Array, RecordBatch, UInt32Array, UInt64Array};
use arrow::datatypes::{DataType, Field, Schema};
use gdb_core::{GdbResult, VertexId};
use gdb_storage::ChunkedCsr;
use std::sync::Arc;

/// Enterprise Graph Analytics Engine (Nebula Enterprise Analytics suite).
pub struct AnalyticsEngine;

impl AnalyticsEngine {
    /// Executes PageRank and returns results as an Arrow RecordBatch.
    pub fn run_pagerank(
        csr: &ChunkedCsr,
        damping: f64,
        max_iter: usize,
        tolerance: f64,
    ) -> GdbResult<RecordBatch> {
        let scores = pagerank(csr, damping, max_iter, tolerance);

        let mut vids = Vec::with_capacity(scores.len());
        let mut score_vals = Vec::with_capacity(scores.len());

        for (vid, score) in scores {
            vids.push(vid.as_u64());
            score_vals.push(score);
        }

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
        let components = weakly_connected_components(csr);

        let mut vids = Vec::with_capacity(components.len());
        let mut comp_ids = Vec::with_capacity(components.len());

        for (vid, comp) in components {
            vids.push(vid.as_u64());
            comp_ids.push(comp.as_u64());
        }

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
        let components = strongly_connected_components(csr);

        let mut vids = Vec::with_capacity(components.len());
        let mut comp_ids = Vec::with_capacity(components.len());

        for (vid, comp) in components {
            vids.push(vid.as_u64());
            comp_ids.push(comp.as_u64());
        }

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
        let comms = louvain(csr, max_iter);

        let mut vids = Vec::with_capacity(comms.len());
        let mut comm_ids = Vec::with_capacity(comms.len());

        for (vid, c) in comms {
            vids.push(vid.as_u64());
            comm_ids.push(c);
        }

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
        let labels = label_propagation(csr, max_iter);

        let mut vids = Vec::with_capacity(labels.len());
        let mut label_ids = Vec::with_capacity(labels.len());

        for (vid, l) in labels {
            vids.push(vid.as_u64());
            label_ids.push(l);
        }

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
        let cores = k_core_decomposition(csr);

        let mut vids = Vec::with_capacity(cores.len());
        let mut core_nums = Vec::with_capacity(cores.len());

        for (vid, k) in cores {
            vids.push(vid.as_u64());
            core_nums.push(k);
        }

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
        let metrics = triangle_count(csr);

        let mut vids = Vec::with_capacity(metrics.len());
        let mut triangles = Vec::with_capacity(metrics.len());
        let mut coeffs = Vec::with_capacity(metrics.len());

        for (vid, m) in metrics {
            vids.push(vid.as_u64());
            triangles.push(m.triangles);
            coeffs.push(m.clustering_coefficient);
        }

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
        let metrics = betweenness_centrality(csr, normalized);

        let mut vids = Vec::with_capacity(metrics.len());
        let mut scores = Vec::with_capacity(metrics.len());

        for (vid, b) in metrics {
            vids.push(vid.as_u64());
            scores.push(b);
        }

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
        let metrics = closeness_centrality(csr);

        let mut vids = Vec::with_capacity(metrics.len());
        let mut scores = Vec::with_capacity(metrics.len());

        for (vid, c) in metrics {
            vids.push(vid.as_u64());
            scores.push(c);
        }

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
        let metrics = degree_centrality(csr);

        let mut vids = Vec::with_capacity(metrics.len());
        let mut in_degs = Vec::with_capacity(metrics.len());
        let mut out_degs = Vec::with_capacity(metrics.len());
        let mut tot_degs = Vec::with_capacity(metrics.len());

        for (vid, m) in metrics {
            vids.push(vid.as_u64());
            in_degs.push(m.in_degree);
            out_degs.push(m.out_degree);
            tot_degs.push(m.total_degree);
        }

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
        let dists = single_source_shortest_path(csr, source);

        let mut vids = Vec::with_capacity(dists.len());
        let mut distances = Vec::with_capacity(dists.len());

        for (vid, d) in dists {
            vids.push(vid.as_u64());
            distances.push(d);
        }

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
    pub fn run_similarity(csr: &ChunkedCsr, node1: VertexId, node2: VertexId) -> GdbResult<RecordBatch> {
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
}
