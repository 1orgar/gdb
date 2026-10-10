import json
import time
from typing import Any, Dict, List, Optional, Tuple, Union
import requests

try:
    import polars as pl
except ImportError:
    pl = None

try:
    import pyarrow as pa
except ImportError:
    pa = None

try:
    import networkx as nx
except ImportError:
    nx = None


class QueryResult:
    """Represents the execution result of a GDB query or batch."""

    def __init__(self, raw: Dict[str, Any]):
        self.raw = raw
        self.status: str = raw.get("status", "ok")
        self.message: str = raw.get("message", "")
        self.error: Optional[str] = raw.get("error")
        self.elapsed_us: int = raw.get("elapsed_us", 0)
        self.num_rows: int = raw.get("num_rows", 0)
        self.rows_affected: int = raw.get("rows_affected", 0)
        self.columns: List[str] = raw.get("columns", [])
        self.rows: List[List[Any]] = raw.get("rows", [])
        self.statements_executed: int = raw.get("statements_executed", 1 if self.status == "ok" else 0)

    @property
    def is_ok(self) -> bool:
        return self.status == "ok"

    def to_polars(self):
        """Convert tabular result to a Polars DataFrame (zero-copy when available)."""
        if pl is None:
            raise ImportError("polars is not installed. Install via `pip install polars`.")
        if not self.columns:
            return pl.DataFrame()
        return pl.DataFrame(self.rows, schema=self.columns, orient="row")

    def to_arrow(self):
        """Convert tabular result to a PyArrow Table."""
        if pa is None:
            raise ImportError("pyarrow is not installed. Install via `pip install pyarrow`.")
        if not self.columns:
            return pa.Table.from_arrays([], names=[])
        pydict = {col: [row[i] for row in self.rows] for i, col in enumerate(self.columns)}
        return pa.Table.from_pydict(pydict)

    def to_pandas(self):
        """Convert tabular result to a Pandas DataFrame."""
        if pl is not None:
            return self.to_polars().to_pandas()
        import pandas as pd
        return pd.DataFrame(self.rows, columns=self.columns)

    def to_networkx(self):
        """Convert graph query results (first 2 columns as source, target) to NetworkX DiGraph."""
        if nx is None:
            raise ImportError("networkx is not installed. Install via `pip install networkx`.")
        G = nx.DiGraph()
        if len(self.columns) < 2:
            return G
        for row in self.rows:
            u, v = row[0], row[1]
            props = {}
            for col_idx, col_name in enumerate(self.columns[2:], start=2):
                props[col_name] = row[col_idx]
            G.add_edge(u, v, **props)
        return G

    def __len__(self) -> int:
        return len(self.rows)

    def __iter__(self):
        return iter(self.rows)

    def __getitem__(self, index):
        return self.rows[index]

    def __repr__(self) -> str:
        if not self.is_ok:
            return f"<QueryResult status=ERROR error={self.error!r}>"
        return f"<QueryResult status=OK rows={len(self.rows)} columns={self.columns} elapsed={self.elapsed_us / 1000.0:.2f}ms>"


class GdbClient:
    """Official Python SDK Client for GDB Graph Database."""

    def __init__(
        self,
        endpoint: str = "http://localhost:8847",
        client_flight_port: int = 8860,
        timeout: float = 30.0,
    ):
        self.endpoint = endpoint.rstrip("/")
        self.client_flight_port = client_flight_port
        self.timeout = timeout
        self.session = requests.Session()

    def __enter__(self):
        return self

    def __exit__(self, exc_type, exc_val, exc_tb):
        self.close()

    def close(self):
        self.session.close()

    def query(self, cypher: str) -> QueryResult:
        """Execute openCypher, GQL, DDL, DML, or CALL query."""
        url = f"{self.endpoint}/query"
        resp = self.session.post(
            url,
            json={"query": cypher},
            timeout=self.timeout,
        )
        resp.raise_for_status()
        res = QueryResult(resp.json())
        if not res.is_ok:
            raise RuntimeError(f"GDB query failed: {res.error}")
        return res

    def execute(self, stmt: str) -> QueryResult:
        """Alias for query()."""
        return self.query(stmt)

    def execute_script(self, script: str) -> QueryResult:
        """Execute a multi-statement Cypher script separated by semicolons."""
        return self.query(script)

    def batch(self, queries: List[str]) -> List[QueryResult]:
        """Execute a list of queries sequentially using /batch endpoint."""
        url = f"{self.endpoint}/batch"
        resp = self.session.post(
            url,
            json={"queries": queries},
            timeout=self.timeout,
        )
        resp.raise_for_status()
        data = resp.json()
        results = [QueryResult(r) for r in data.get("results", [])]
        return results

    def insert_vertices(
        self,
        label: str,
        data: Union[List[Dict[str, Any]], Any],
        batch_size: int = 500,
    ) -> int:
        """Batch insert vertices using multi-row VALUES syntax."""
        if pl is not None and isinstance(data, pl.DataFrame):
            dicts = data.to_dicts()
        elif hasattr(data, "to_dict"):
            dicts = data.to_dict(orient="records")
        else:
            dicts = list(data)

        if not dicts:
            return 0

        total_affected = 0
        props = [k for k in dicts[0].keys()]
        prop_list_str = ", ".join(props)

        for i in range(0, len(dicts), batch_size):
            chunk = dicts[i : i + batch_size]
            val_strs = []
            for item in chunk:
                vals = []
                for p in props:
                    val = item[p]
                    if val is None:
                        vals.append("null")
                    elif isinstance(val, str):
                        vals.append(f"'{val.replace('\'', '\\\'')}'")
                    elif isinstance(val, (list, tuple)):
                        vals.append(f"[{', '.join(str(float(x)) for x in val)}]")
                    elif isinstance(val, bool):
                        vals.append("true" if val else "false")
                    else:
                        vals.append(str(val))
                val_strs.append(f"({', '.join(vals)})")

            query = f"INSERT VERTEX {label} ({prop_list_str}) VALUES {', '.join(val_strs)};"
            res = self.query(query)
            total_affected += res.rows_affected or len(chunk)

        return total_affected

    def insert_edges(
        self,
        edge_type: str,
        edges: Union[List[Tuple[int, int]], Any],
        batch_size: int = 500,
    ) -> int:
        """Batch insert edges using multi-row syntax or batch statements."""
        if hasattr(edges, "to_numpy"):
            arr = edges.to_numpy()
            edge_list = [(int(row[0]), int(row[1])) for row in arr]
        else:
            edge_list = list(edges)

        if not edge_list:
            return 0

        total_affected = 0
        for i in range(0, len(edge_list), batch_size):
            chunk = edge_list[i : i + batch_size]
            statements = [
                f"INSERT EDGE {edge_type} FROM {u} TO {v};"
                for u, v in chunk
            ]
            batch_script = " ".join(statements)
            res = self.query(batch_script)
            total_affected += res.rows_affected or len(chunk)

        return total_affected

    def compact(self) -> Dict[str, Any]:
        """Trigger asynchronous background CSR compaction."""
        url = f"{self.endpoint}/compact"
        resp = self.session.post(url, timeout=self.timeout)
        resp.raise_for_status()
        return resp.json()

    def analyze(self) -> QueryResult:
        """Compute cardinality and degree statistics for Cost-Based Optimizer (CBO)."""
        return self.query("ANALYZE GRAPH;")

    def health(self) -> Dict[str, Any]:
        """Get node and cluster health."""
        resp = self.session.get(f"{self.endpoint}/health", timeout=self.timeout)
        resp.raise_for_status()
        return resp.json()

    def cluster(self) -> Dict[str, Any]:
        """Get cluster hash ring topology and node replica mappings."""
        resp = self.session.get(f"{self.endpoint}/cluster", timeout=self.timeout)
        resp.raise_for_status()
        return resp.json()

    def schema(self) -> Dict[str, Any]:
        """Get registered vertex labels, edge types, and properties."""
        resp = self.session.get(f"{self.endpoint}/schema", timeout=self.timeout)
        resp.raise_for_status()
        return resp.json()

    def resources(self) -> Dict[str, Any]:
        """Get live memory, CSR edge count, and Delta MemTable statistics."""
        resp = self.session.get(f"{self.endpoint}/resources", timeout=self.timeout)
        resp.raise_for_status()
        return resp.json()

    def gpu(self) -> Dict[str, Any]:
        """Get GPU acceleration status, device ID, memory architecture, and kernels."""
        resp = self.session.get(f"{self.endpoint}/gpu", timeout=self.timeout)
        resp.raise_for_status()
        return resp.json()
