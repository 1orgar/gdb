// ============================================================
// GDB Example: Social Network Graph (openCypher / GQL)
// ============================================================

// 1. Schema Definition (DDL)
CREATE VERTEX User (name STRING, age INT64);
CREATE EDGE FOLLOWS ();

// 2. Insert Users (DML)
INSERT VERTEX User (id, name, age) VALUES (1, 'Alice', 28);
INSERT VERTEX User (id, name, age) VALUES (2, 'Bob', 34);
INSERT VERTEX User (id, name, age) VALUES (3, 'Charlie', 22);
INSERT VERTEX User (id, name, age) VALUES (4, 'Diana', 29);
INSERT VERTEX User (id, name, age) VALUES (5, 'Evan', 31);
INSERT VERTEX User (id, name, age) VALUES (6, 'Fiona', 26);

// 3. Insert Follows Relationships
INSERT EDGE FOLLOWS FROM 1 TO 2;
INSERT EDGE FOLLOWS FROM 1 TO 3;
INSERT EDGE FOLLOWS FROM 2 TO 3;
INSERT EDGE FOLLOWS FROM 2 TO 4;
INSERT EDGE FOLLOWS FROM 3 TO 1;
INSERT EDGE FOLLOWS FROM 3 TO 4;
INSERT EDGE FOLLOWS FROM 4 TO 5;
INSERT EDGE FOLLOWS FROM 5 TO 6;
INSERT EDGE FOLLOWS FROM 6 TO 4;

// 4. Compact into Chunked-CSR for optimal traversal speed
compact;

// 5. 1-Hop Pattern Matching
MATCH (u:User)-[:FOLLOWS]->(target:User)
RETURN u.name, target.name;

// 6. 2-Hop Traversal (Friends of Friends)
MATCH (u:User)-[:FOLLOWS]->(f:User)-[:FOLLOWS]->(fof:User)
RETURN u.name, f.name, fof.name;

// 7. Graph Analytics: PageRank (Influence Ranking)
CALL algo.pageRank({damping: 0.85, max_iter: 20}) YIELD vertex_id, score;

// 8. Graph Analytics: Louvain Modularity (Community Detection)
CALL algo.louvain({max_iter: 10}) YIELD vertex_id, community_id;

// 9. Graph Analytics: Triangle Counting (Clustering)
CALL algo.triangleCount() YIELD vertex_id, triangles;
