use crate::state::RingNode;

pub fn parse_peer_host_and_port(peer: &str) -> (String, u16) {
    let trimmed = peer.trim_start_matches("http://").trim_start_matches("https://");
    let host_port = trimmed.split('/').next().unwrap_or(trimmed);
    let mut parts = host_port.split(':');
    let host = parts.next().unwrap_or("127.0.0.1").to_string();
    let port = parts.next().and_then(|p| p.parse::<u16>().ok()).unwrap_or(8847);
    (if host.is_empty() { "127.0.0.1".to_string() } else { host }, port)
}

pub fn build_ring_nodes(
    my_id: u64,
    my_http_port: u16,
    my_flight_address: &str,
    my_flight_port: u16,
    my_client_flight_address: &str,
    my_client_flight_port: u16,
    peers: &[String],
) -> Vec<RingNode> {
    let mut nodes = Vec::new();
    nodes.push(RingNode {
        node_id: my_id,
        http_url: format!("http://{}:{}", my_client_flight_address, my_http_port),
        flight_address: my_flight_address.to_string(),
        flight_port: my_flight_port,
        client_flight_address: my_client_flight_address.to_string(),
        client_flight_port: my_client_flight_port,
    });

    for (idx, peer) in peers.iter().enumerate() {
        let (peer_host, _peer_http_port) = parse_peer_host_and_port(peer);
        let (peer_id, flight_p, client_p) = if peer.contains("8847") {
            (1, 8848, 8860)
        } else if peer.contains("8846") {
            (2, 8849, 8861)
        } else if peer.contains("8845") {
            (3, 8850, 8862)
        } else {
            let id = if (idx as u64 + 1) >= my_id {
                idx as u64 + 2
            } else {
                idx as u64 + 1
            };
            (id, 8848 + id as u16, 8860 + (id as u16).saturating_sub(1))
        };
        nodes.push(RingNode {
            node_id: peer_id,
            http_url: peer.clone(),
            flight_address: peer_host.clone(),
            flight_port: flight_p,
            client_flight_address: peer_host,
            client_flight_port: client_p,
        });
    }

    nodes.sort_by_key(|n| n.node_id);
    nodes.dedup_by_key(|n| n.node_id);
    nodes
}

pub fn calculate_replica_set(key: u64, ring: &[RingNode], rf: u32) -> Vec<RingNode> {
    if ring.is_empty() {
        return Vec::new();
    }
    let n = ring.len();
    let primary_idx = (key as usize) % n;
    let effective_rf = (rf as usize).min(n).max(1);

    let mut replicas = Vec::with_capacity(effective_rf);
    for i in 0..effective_rf {
        let idx = (primary_idx + i) % n;
        replicas.push(ring[idx].clone());
    }
    replicas
}
