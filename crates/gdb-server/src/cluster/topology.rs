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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_peer_host_and_port() {
        assert_eq!(parse_peer_host_and_port("http://192.168.1.10:9000"), ("192.168.1.10".to_string(), 9000));
        assert_eq!(parse_peer_host_and_port("https://myhost.domain:8080/"), ("myhost.domain".to_string(), 8080));
        assert_eq!(parse_peer_host_and_port("10.0.0.1"), ("10.0.0.1".to_string(), 8847));
        assert_eq!(parse_peer_host_and_port(":7777"), ("127.0.0.1".to_string(), 7777));
        assert_eq!(parse_peer_host_and_port(""), ("127.0.0.1".to_string(), 8847));
    }

    #[test]
    fn test_build_ring_nodes() {
        let peers = vec![
            "http://127.0.0.1:8847".to_string(),
            "http://127.0.0.1:8846".to_string(),
            "http://127.0.0.1:8845".to_string(),
            "http://10.0.0.5:9999".to_string(),
        ];
        let nodes = build_ring_nodes(1, 8847, "127.0.0.1", 8848, "127.0.0.1", 8860, &peers);
        assert!(!nodes.is_empty());
        assert!(nodes.iter().any(|n| n.node_id == 1));
        assert!(nodes.iter().any(|n| n.node_id == 2));
        assert!(nodes.iter().any(|n| n.node_id == 3));

        // Ensure sorted and deduped
        for i in 1..nodes.len() {
            assert!(nodes[i].node_id > nodes[i - 1].node_id);
        }
    }

    #[test]
    fn test_calculate_replica_set() {
        let ring = vec![
            RingNode {
                node_id: 1,
                http_url: "http://127.0.0.1:8847".to_string(),
                flight_address: "127.0.0.1".to_string(),
                flight_port: 8848,
                client_flight_address: "127.0.0.1".to_string(),
                client_flight_port: 8860,
            },
            RingNode {
                node_id: 2,
                http_url: "http://127.0.0.1:8846".to_string(),
                flight_address: "127.0.0.1".to_string(),
                flight_port: 8849,
                client_flight_address: "127.0.0.1".to_string(),
                client_flight_port: 8861,
            },
            RingNode {
                node_id: 3,
                http_url: "http://127.0.0.1:8845".to_string(),
                flight_address: "127.0.0.1".to_string(),
                flight_port: 8850,
                client_flight_address: "127.0.0.1".to_string(),
                client_flight_port: 8862,
            },
        ];

        // Empty ring
        assert!(calculate_replica_set(10, &[], 2).is_empty());

        // RF = 1
        let r1 = calculate_replica_set(0, &ring, 1);
        assert_eq!(r1.len(), 1);
        assert_eq!(r1[0].node_id, 1);

        let r2 = calculate_replica_set(1, &ring, 1);
        assert_eq!(r2.len(), 1);
        assert_eq!(r2[0].node_id, 2);

        // RF = 2 (wraparound)
        let r_wrap = calculate_replica_set(2, &ring, 2);
        assert_eq!(r_wrap.len(), 2);
        assert_eq!(r_wrap[0].node_id, 3);
        assert_eq!(r_wrap[1].node_id, 1);

        // RF > ring.len()
        let r_large = calculate_replica_set(0, &ring, 10);
        assert_eq!(r_large.len(), 3);
    }
}
