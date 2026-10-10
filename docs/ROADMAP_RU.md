# GDB: Дорожная карта развития (Roadmap)

Документ определяет приоритеты и план развития высокопроизводительной графовой СУБД **GDB**.

---

## 📌 Текущий статус (Версия v0.5.0 — Vermeer)

- ✅ **Multi-Statement & Relationship DML**:
  - Лексический сплиттер скриптов (`split_statements`), корректно обрабатывающий строковые кавычки и комментарии.
  - Исполнение скриптов через `/query` и выделенный эндпоинт `POST /batch`.
  - Cypher Relationship DML: `MATCH (a:Tag), (b:Tag) CREATE (a)-[r:TYPE]->(b)` и `MERGE (a)-[r:TYPE]->(b)`.
  - GDB Studio UI: кнопка «Run All» с пошаговым индикатором прогресса (`Step: X / Y`) и подсветкой ошибок.
  - GDB CLI: пакетное выполнение команд в локальном и кластерном режимах.
- ✅ **Нативные векторные эмбеддинги и поиск сходства**:
  - Нативный тип `VECTOR(dim)` с колоночным представлением Apache Arrow `FixedSizeList`.
  - Парсинг векторных литералов `[1.0, 2.0, 3.0]`.
  - Процедура `CALL vector.similaritySearch(label, property, query_vector, k, metric)` с поддержкой метрик Cosine, DotProduct и Euclidean L2.
- ✅ **GPU Search Offload & Ускорение графа**:
  - Перенос волнового обхода BFS (`VarLengthExpand`) на GPU (Apple Metal UMA / NVIDIA CUDA) с автоматическим Rayon CPU fallback.
  - Векторизованный параллельный расчет векторного сходства на SIMD и GPU.
  - Полный пакет GPU-алгоритмов аналитики (PageRank, Louvain, WCC, Triangle Counting, SSSP).
- ✅ **CBO Optimizer & Graph ML**:
  - Сбор статистики графа для Cost-Based Optimizer через `ANALYZE GRAPH;`.
  - Генерация эмбеддингов случайными блужданиями и Skip-Gram Node2Vec (`CALL algo.node2vec(...)`).
- ✅ **Официальный Python SDK & Клиентский бенчмарк**:
  - Автономный пакет `gdb-client` для Python с экспортом в Polars и NetworkX.
  - Единый файл зависимостей `scripts/requirements.txt`.
  - Высокоскоростной бенчмарк `scripts/py_client_benchmark.py` с автоматическим управлением схемой (`--keep-schema`).
  - Пачечные вставки (`VALUES (...), (...)`) в `data_loader.py`, `gpu_benchmark.py`, `stress_test.py` и `benchmark_suite.py`.
- ✅ **Качество кода и покрытие тестами**:
  - 100% успешных тестов во всех 13 крейтах воркспейса.
  - Тестовое покрытие $\ge 80\%$ (**80.50% строк / 80.60% регионов**).

---

## 📅 Дорожная карта релизов

| Релиз | Кодовое имя | Главный фокус | Ключевые возможности |
| :---: | :---: | :--- | :--- |
| **v0.5.0** | **Vermeer** *(Текущий)* | **Multi-Statement, Векторы, GPU Offload, CBO & Python SDK** | Все ключевые задачи релиза v0.5.0 успешно реализованы и протестированы. |
| **v0.6.0** | **Hals** | **Интеграция с AI Агентами & Расширенное индексирование** | 1. Официальные модули интеграции с экосистемами **LangChain** и **LangGraph** (`langchain-gdb`, GraphVectorStore).<br>2. HNSW-индекс графа векторов для субмиллисекундного приближенного поиска на миллиардных датасетах.<br>3. Гибридные цепочки Graph RAG, объединяющие многошаговые обходы графа с семантическим векторным ранжированием. |
| **v0.7.0** | **Steen** | **ACID-транзакции & Унифицированный пейджинг памяти** | 1. Распределенные многопартиционные ACID-транзакции (`BEGIN`, `COMMIT`, `ROLLBACK`) с 2PC и Snapshot Isolation.<br>2. GPU Unified Memory Paging (UVM) для графов, превышающих физическую память дискретных GPU.<br>3. Алгоритмы поиска взвешенных кратчайших путей (Dijkstra, A*). |
| **v1.0.0** | **Erasmus** | **Корпоративная безопасность & LTS** | 1. Шифрование TLS / mTLS и JWT-аутентификация.<br>2. Гранулярный ролевой доступ (RBAC) на уровне меток и свойств.<br>3. Потоковый `InstallSnapshot` через Arrow Flight RPC в Multi-Raft.<br>4. Релиз с долгосрочной поддержкой (LTS) и строгой гарантией обратной совместимости API. |

---

## 🎯 Архитектурные особенности (v0.5.0)

### 1. Гибридный графо-векторный поиск
Векторные свойства хранятся непосредственно в колонках Apache Arrow `FixedSizeList` вместе с остальными атрибутами вершин. Это обеспечивает прозрачное комбинирование поиска по графовым паттернам и семантического сходства:
```cypher
MATCH (u:User {dept: 'Research'})-[:AUTHORED]->(d:Document)
CALL vector.similaritySearch('Document', 'embedding', [0.12, 0.45, -0.33], 5, 'cosine')
YIELD vertex_id, score
RETURN d.title, score ORDER BY score DESC;
```

### 2. Максимальный перенос графовых операций на GPU
Графовые операции автоматически маршрутизируются через `QueryExecutor::with_gpu`. При превышении порога размера графа многошаговые обходы (`VarLengthExpand`) выполняются на ядрах GPU с нулевыми накладными расходами на копирование благодаря архитектуре UMA на Apple Silicon.

### 3. Официальный Python SDK (`gdb-client`)
```python
from gdb_client import GdbClient

client = GdbClient(endpoint="http://127.0.0.1:8847")
df = client.query_df("MATCH (p:Person) RETURN p.name, p.age;")
G = client.query_graph("MATCH (a:Person)-[r:KNOWS]->(b:Person) RETURN a.id, b.id, r.weight;")
```
