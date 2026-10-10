# GDB Python Client (`gdb-client`)

Official Python SDK for **GDB** — Distributed In-Memory Graph Database.

## Features

- **High-Performance Querying**: Execute openCypher, GQL, and graph algorithm queries (`CALL algo.*`).
- **Multi-Statement Script Execution**: Run semicolon-separated Cypher scripts seamlessly.
- **Batch Processing**: Sized multi-row batch vertex/edge ingestions.
- **DataFrame Integration**: Direct conversion to **Polars** (`result.to_polars()`) and **Pandas** (`result.to_pandas()`).
- **NetworkX Integration**: Export graph query subgraphs directly to **NetworkX** (`result.to_networkx()`).
- **Vector Search & Embeddings**: Native support for `VECTOR(dim)` types, cosine/euclidean similarity, and Node2Vec embeddings.

## Installation

```bash
pip install gdb-client
# Or with all optional analytics dependencies:
pip install "gdb-client[all]"
```

## Quick Start

```python
from gdb_client import GdbClient

client = GdbClient("http://localhost:8847")

# 1. Multi-statement schema setup
client.execute_script("""
    CREATE VERTEX Article (title STRING, emb VECTOR(3));
    INSERT VERTEX Article (id, title, emb) VALUES (1, 'Graph ML', [1.0, 0.0, 0.0]);
    INSERT VERTEX Article (id, title, emb) VALUES (2, 'Vector DB', [0.9, 0.1, 0.0]);
""")

# 2. Similarity Search
res = client.query("CALL vector.similaritySearch('Article', 'emb', [1.0, 0.0, 0.0], 5, 'cosine') YIELD vertex_id, score;")
print(res.to_polars())

# 3. Analyze Graph for Cost-Based Optimizer
client.analyze()
```
