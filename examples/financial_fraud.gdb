// ============================================================
// GDB Example: Financial Fraud Detection (Money Laundering Rings)
// ============================================================

// 1. Schema Definition
CREATE VERTEX Account (holder STRING, balance FLOAT64);
CREATE EDGE TRANSFERRED ();

// 2. Insert Accounts
INSERT VERTEX Account (id, holder, balance) VALUES (101, 'Entity_A', 150000.0);
INSERT VERTEX Account (id, holder, balance) VALUES (102, 'Mule_1', 48000.0);
INSERT VERTEX Account (id, holder, balance) VALUES (103, 'Mule_2', 47500.0);
INSERT VERTEX Account (id, holder, balance) VALUES (104, 'Mule_3', 49000.0);
INSERT VERTEX Account (id, holder, balance) VALUES (105, 'Offshore_Shell', 142000.0);

// Legitimate Accounts
INSERT VERTEX Account (id, holder, balance) VALUES (201, 'Corporate_Payroll', 500000.0);
INSERT VERTEX Account (id, holder, balance) VALUES (202, 'Employee_John', 4500.0);
INSERT VERTEX Account (id, holder, balance) VALUES (203, 'Employee_Sarah', 6200.0);

// 3. Insert Money Transfers (Smurfing / Circular Laundering Ring)
INSERT EDGE TRANSFERRED FROM 101 TO 102;
INSERT EDGE TRANSFERRED FROM 101 TO 103;
INSERT EDGE TRANSFERRED FROM 101 TO 104;
INSERT EDGE TRANSFERRED FROM 102 TO 105;
INSERT EDGE TRANSFERRED FROM 103 TO 105;
INSERT EDGE TRANSFERRED FROM 104 TO 105;
// Circular kickback flow to obscure origin
INSERT EDGE TRANSFERRED FROM 105 TO 101;

// Legitimate payroll transfers
INSERT EDGE TRANSFERRED FROM 201 TO 202;
INSERT EDGE TRANSFERRED FROM 201 TO 203;

// 4. Compact into Chunked-CSR
compact;

// 5. Detect 2-Hop Layering Paths
MATCH (src:Account)-[:TRANSFERRED]->(mule:Account)-[:TRANSFERRED]->(dest:Account)
RETURN src.holder, mule.holder, dest.holder;

// 6. Partition Graph into Isolated Communities (WCC)
CALL algo.wcc() YIELD vertex_id, component_id;

// 7. Detect Circular Hubs & Triangles (Synthetic Fraud Rings)
CALL algo.triangleCount() YIELD vertex_id, triangles;

// 8. Find Shortest Path from Source to Shell Entity (SSSP)
CALL algo.sssp({source: 101}) YIELD vertex_id, distance;

// 9. Node Similarity between Mules
CALL algo.similarity({node1: 102, node2: 103}) YIELD jaccard, common_neighbors;
