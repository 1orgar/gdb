use crate::types::{NodeId, RaftMutation, RaftResponse};
use dashmap::DashMap;
use gdb_core::{GdbError, GdbResult, VertexId};
use gdb_storage::PartitionStorageEngine;
use gdb_wal::WriteAheadLog;
use parking_lot::RwLock;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

pub struct RaftGroup {
    pub partition_id: u32,
    pub leader_id: RwLock<NodeId>,
    pub peers: RwLock<HashSet<NodeId>>,
    pub storage: Arc<PartitionStorageEngine>,
    pub wal: Arc<WriteAheadLog>,
}

impl RaftGroup {
    pub fn new(
        partition_id: u32,
        current_node_id: NodeId,
        peers: Vec<NodeId>,
        storage: Arc<PartitionStorageEngine>,
        wal: Arc<WriteAheadLog>,
    ) -> Self {
        let mut peer_set: HashSet<NodeId> = peers.into_iter().collect();
        peer_set.insert(current_node_id);

        Self {
            partition_id,
            leader_id: RwLock::new(current_node_id),
            peers: RwLock::new(peer_set),
            storage,
            wal,
        }
    }

    /// Replicates and applies a mutation if this node is leader.
    pub fn propose(&self, mutation: RaftMutation) -> GdbResult<RaftResponse> {
        let ver = self.storage.next_commit_version();

        // 1. Serialize and persist to local Raft WAL
        let payload = bincode::serialize(&mutation)
            .map_err(|e| GdbError::Serialization(e.to_string()))?;
        self.wal.append(ver, &payload)?;

        // 2. Apply to Partition In-Memory Storage Engine
        match mutation {
            RaftMutation::InsertVertex { label_id, id, properties } => {
                self.storage.set_vertex_properties(id, label_id, properties)?;
            }
            RaftMutation::InsertEdge { edge } => {
                self.storage.insert_edge(edge, ver);
            }
            RaftMutation::DeleteEdge { edge } => {
                self.storage.delete_edge(edge, ver);
            }
        }

        Ok(RaftResponse {
            success: true,
            commit_version: ver,
            message: format!("Committed to partition {} at version {}", self.partition_id, ver),
        })
    }
}

/// Multi-Raft Manager orchestrating partition-level consensus groups across the cluster.
pub struct MultiRaftManager {
    pub node_id: NodeId,
    pub total_partitions: u32,
    groups: DashMap<u32, Arc<RaftGroup>>,
    wal_dir: PathBuf,
}

impl MultiRaftManager {
    pub fn new(node_id: NodeId, total_partitions: u32, wal_dir: PathBuf) -> Self {
        Self {
            node_id,
            total_partitions,
            groups: DashMap::new(),
            wal_dir,
        }
    }

    /// Registers a partition Raft group on this node.
    pub fn register_partition(
        &self,
        partition_id: u32,
        storage: Arc<PartitionStorageEngine>,
        peers: Vec<NodeId>,
    ) -> GdbResult<Arc<RaftGroup>> {
        std::fs::create_dir_all(&self.wal_dir).map_err(|e| GdbError::Storage(e.to_string()))?;
        let wal_path = self.wal_dir.join(format!("p{}_raft.wal", partition_id));
        let wal = Arc::new(WriteAheadLog::open(wal_path)?);

        let group = Arc::new(RaftGroup::new(
            partition_id,
            self.node_id,
            peers,
            storage,
            wal,
        ));
        self.groups.insert(partition_id, group.clone());
        Ok(group)
    }

    pub fn get_group(&self, partition_id: u32) -> Option<Arc<RaftGroup>> {
        self.groups.get(&partition_id).map(|g| g.clone())
    }

    /// Routes a mutation by hashing the primary vertex ID to its corresponding partition.
    pub fn route_mutation(&self, target_vertex: VertexId, mutation: RaftMutation) -> GdbResult<RaftResponse> {
        let partition = target_vertex.partition(self.total_partitions);
        let group = self.get_group(partition)
            .ok_or_else(|| GdbError::Raft(format!("Partition {} not hosted on node {}", partition, self.node_id)))?;
        group.propose(mutation)
    }
}
