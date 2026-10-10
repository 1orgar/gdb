"""
GDB Python Client
=================
Official Python SDK for GDB (Distributed In-Memory Graph Database).
"""

from .client import GdbClient, QueryResult

__version__ = "0.5.1"
__all__ = ["GdbClient", "QueryResult"]
