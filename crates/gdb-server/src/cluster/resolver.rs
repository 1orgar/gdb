use crate::cluster::topology::calculate_replica_set;
use crate::state::{EdgeDto, RingNode};
use gdb_core::{DataValue, EdgeId, EdgeType, LabelId, VertexId};
use gdb_planner::{RemoteEdgeExpander, RemotePropertyResolver};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

pub struct ClusterEdgeExpander {
    pub ring_nodes: Arc<RwLock<Vec<RingNode>>>,
    pub node_id: u64,
    pub http_client: reqwest::Client,
    pub replication_factor: u32,
    pub cache: Arc<RwLock<HashMap<VertexId, Vec<EdgeId>>>>,
}

impl RemoteEdgeExpander for ClusterEdgeExpander {
    fn get_out_edges(&self, vid: VertexId, edge_type: Option<EdgeType>) -> Vec<EdgeId> {
        let batch_map = self.batch_get_out_edges(&[vid], edge_type);
        batch_map.get(&vid).cloned().unwrap_or_default()
    }

    fn batch_get_out_edges(&self, vids: &[VertexId], edge_type: Option<EdgeType>) -> HashMap<VertexId, Vec<EdgeId>> {
        let mut results: HashMap<VertexId, Vec<EdgeId>> = HashMap::new();
        let mut missing: Vec<VertexId> = Vec::new();

        {
            let cache = self.cache.read();
            for &vid in vids {
                if let Some(edges) = cache.get(&vid) {
                    let filtered: Vec<EdgeId> = if let Some(et) = edge_type {
                        edges.iter().filter(|e| e.edge_type == et).cloned().collect()
                    } else {
                        edges.clone()
                    };
                    results.insert(vid, filtered);
                } else {
                    missing.push(vid);
                }
            }
        }

        if missing.is_empty() {
            return results;
        }

        let ring_snapshot = self.ring_nodes.read().clone();
        if ring_snapshot.len() <= 1 {
            return results;
        }

        let mut peer_groups: HashMap<String, Vec<u64>> = HashMap::new();
        for vid in missing {
            let replica_set = calculate_replica_set(vid.0, &ring_snapshot, self.replication_factor);
            if let Some(target) = replica_set.into_iter().find(|n| n.node_id != self.node_id) {
                peer_groups.entry(target.http_url.trim_end_matches('/').to_string())
                    .or_default()
                    .push(vid.0);
            } else {
                for peer in ring_snapshot.iter().filter(|n| n.node_id != self.node_id) {
                    peer_groups.entry(peer.http_url.trim_end_matches('/').to_string())
                        .or_default()
                        .push(vid.0);
                }
            }
        }

        if peer_groups.is_empty() {
            return results;
        }

        let client = self.http_client.clone();
        let et_u32 = edge_type.map(|e| e.0);

        let do_fetch = |pg: HashMap<String, Vec<u64>>, c: reqwest::Client, et: Option<u32>| async move {
            let mut futs = Vec::new();
            for (peer_url, vids_list) in pg {
                let client = c.clone();
                let payload = serde_json::json!({
                    "vids": vids_list,
                    "edge_type": et,
                });
                let url = format!("{}/storage/out_edges", peer_url);
                futs.push(async move {
                    match client.post(&url)
                        .json(&payload)
                        .timeout(std::time::Duration::from_millis(2000))
                        .send()
                        .await
                    {
                        Ok(resp) => resp.json::<HashMap<u64, Vec<EdgeDto>>>().await.unwrap_or_default(),
                        Err(e) => {
                            tracing::debug!("Remote edge lookup at {} failed: {}", url, e);
                            HashMap::new()
                        }
                    }
                });
            }
            let mut combined = HashMap::new();
            for map in futures::future::join_all(futs).await {
                combined.extend(map);
            }
            combined
        };

        let fetched_map: HashMap<u64, Vec<EdgeDto>> = if let Ok(handle) = tokio::runtime::Handle::try_current() {
            match handle.runtime_flavor() {
                tokio::runtime::RuntimeFlavor::MultiThread => {
                    tokio::task::block_in_place(|| {
                        handle.block_on(do_fetch(peer_groups, client, et_u32))
                    })
                }
                _ => {
                    std::thread::spawn(move || {
                        let rt = tokio::runtime::Builder::new_current_thread()
                            .enable_all()
                            .build()
                            .unwrap();
                        rt.block_on(do_fetch(peer_groups, client, et_u32))
                    })
                    .join()
                    .unwrap_or_default()
                }
            }
        } else {
            std::thread::spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap();
                rt.block_on(do_fetch(peer_groups, client, et_u32))
            })
            .join()
            .unwrap_or_default()
        };

        {
            let mut cache = self.cache.write();
            for (vid_u64, edge_dtos) in fetched_map {
                let vid = VertexId(vid_u64);
                let edge_ids: Vec<EdgeId> = edge_dtos.into_iter().map(EdgeId::from).collect();
                cache.insert(vid, edge_ids.clone());
                let filtered: Vec<EdgeId> = if let Some(et) = edge_type {
                    edge_ids.into_iter().filter(|e| e.edge_type == et).collect()
                } else {
                    edge_ids
                };
                results.insert(vid, filtered);
            }
        }

        results
    }
}

impl RemotePropertyResolver for ClusterEdgeExpander {
    fn batch_get_vertex_properties(
        &self,
        vids: &[VertexId],
        label_id: Option<LabelId>,
        properties: Option<&[String]>,
    ) -> HashMap<VertexId, HashMap<String, DataValue>> {
        if vids.is_empty() {
            return HashMap::new();
        }

        let ring_snapshot = self.ring_nodes.read().clone();
        if ring_snapshot.len() <= 1 {
            return HashMap::new();
        }

        let mut peer_groups: HashMap<String, Vec<u64>> = HashMap::new();
        for &vid in vids {
            let replica_set = calculate_replica_set(vid.0, &ring_snapshot, self.replication_factor);
            if let Some(target) = replica_set.into_iter().find(|n| n.node_id != self.node_id) {
                peer_groups.entry(target.http_url.trim_end_matches('/').to_string())
                    .or_default()
                    .push(vid.0);
            } else {
                for peer in ring_snapshot.iter().filter(|n| n.node_id != self.node_id) {
                    peer_groups.entry(peer.http_url.trim_end_matches('/').to_string())
                        .or_default()
                        .push(vid.0);
                }
            }
        }

        if peer_groups.is_empty() {
            return HashMap::new();
        }

        let client = self.http_client.clone();
        let lid_u32 = label_id.map(|l| l.0);
        let props_vec = properties.map(|p| p.to_vec());

        let do_fetch = |pg: HashMap<String, Vec<u64>>, c: reqwest::Client, lid: Option<u32>, p_list: Option<Vec<String>>| async move {
            let mut futs = Vec::new();
            for (peer_url, vids_list) in pg {
                let client = c.clone();
                let payload = serde_json::json!({
                    "vids": vids_list,
                    "label_id": lid,
                    "properties": p_list,
                });
                let url = format!("{}/storage/vertex_properties", peer_url);
                futs.push(async move {
                    match client.post(&url)
                        .json(&payload)
                        .timeout(std::time::Duration::from_millis(2000))
                        .send()
                        .await
                    {
                        Ok(resp) => resp.json::<HashMap<u64, HashMap<String, DataValue>>>().await.unwrap_or_default(),
                        Err(e) => {
                            tracing::debug!("Remote vertex properties lookup at {} failed: {}", url, e);
                            HashMap::new()
                        }
                    }
                });
            }
            let mut combined = HashMap::new();
            for map in futures::future::join_all(futs).await {
                for (vid_u64, prop_map) in map {
                    combined.insert(VertexId(vid_u64), prop_map);
                }
            }
            combined
        };

        let fetched_map: HashMap<VertexId, HashMap<String, DataValue>> = if let Ok(handle) = tokio::runtime::Handle::try_current() {
            match handle.runtime_flavor() {
                tokio::runtime::RuntimeFlavor::MultiThread => {
                    tokio::task::block_in_place(|| {
                        handle.block_on(do_fetch(peer_groups, client, lid_u32, props_vec))
                    })
                }
                _ => {
                    std::thread::spawn(move || {
                        let rt = tokio::runtime::Builder::new_current_thread()
                            .enable_all()
                            .build()
                            .unwrap();
                        rt.block_on(do_fetch(peer_groups, client, lid_u32, props_vec))
                    })
                    .join()
                    .unwrap_or_default()
                }
            }
        } else {
            std::thread::spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap();
                rt.block_on(do_fetch(peer_groups, client, lid_u32, props_vec))
            })
            .join()
            .unwrap_or_default()
        };

        fetched_map
    }
}
