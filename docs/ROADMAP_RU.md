# GDB: Дорожная карта развития (Roadmap)

Документ определяет приоритеты и план развития высокопроизводительной графовой СУБД **GDB**.

---

## 📌 Текущий статус (Версия v0.4.2 — Huygens)

- ✅ **openCypher `WITH` Pipeline**: промежуточные проекции, агрегации (`count`, `sum`, `avg`, `min`, `max`), фильтрация `WHERE`, сортировка `ORDER BY`, пагинация `SKIP`/`LIMIT`.
- ✅ **Schema Manager и чистый граф в GDB Studio**: отсутствие навязанных дефолтных вершин, динамический DDL просмотр и удаление/изменение схем (`DROP`/`ALTER`).
- ✅ **Регистрация NVIDIA GPU в `nvidia-smi`**: динамический контекст CUDA Driver API и выделение VRAM-буфера с флагом `Type: C`.
- ✅ **Тестовое покрытие >= 80% в CI**: **81.62%** покрытие строк во всём воркспейсе (`cargo llvm-cov`).
- ✅ **Автоматический жизненный цикл схем в тестах**: автоматическое создание и удаление тестовых топологий во всех скриптах с опцией `--keep-schema`.
- ✅ **Выделенный GPU бенчмарк**: `scripts/gpu_benchmark.py` (Metal UMA / NVIDIA CUDA / CPU fallback).
- ✅ **Документация**: подробные руководства по тестированию и бенчмаркам в `docs/TESTING_AND_BENCHMARKS.md` и `docs/TESTING_AND_BENCHMARKS_RU.md`.

---

## 📅 Дорожная карта релизов

| Релиз | Кодовое имя | Главный фокус | Ключевые возможности |
| :---: | :---: | :--- | :--- |
| **v0.5.0** | **Vermeer** | **Multi-Statement & Relationship DML** | 1. Выполнение пачек запросов через `;` в Studio, CLI и эндпоинте `POST /batch`.<br>2. `MATCH ... CREATE (a)-[r:TYPE]->(b)` с атрибутами связей.<br>3. `MERGE (a)-[r:TYPE]->(b) ON CREATE SET ...`.<br>4. Оптимизация `data_loader.py` и сидеров на batch-insert (`VALUES (...), (...)`). |
| **v0.6.0** | **Hals** | **GPU Search Offload & Vector Embeddings** | 1. **GPU Offload для поиска**: многошаговый обход путей (`VarLengthExpand` / BFS) на GPU (Metal UMA / CUDA) и параллельная предикатная фильтрация.<br>2. Нативный тип `VECTOR(dim)` в схеме и Arrow-хранилище.<br>3. HNSW-индекс векторного поиска с аппаратным ускорением SIMD/GPU.<br>4. Процедура гибридного поиска `CALL vector.similaritySearch(...)`. |
| **v0.7.0** | **Steen** | **Python SDK & Client Benchmark** | 1. Официальный пакет `gdb-py-client` с поддержкой Arrow Flight RPC и Polars.<br>2. Скрипт бенчмарка `scripts/py_client_benchmark.py` (Flight vs HTTP, latency percentiles).<br>3. Zero-Copy интеграция с PyTorch Geometric и NetworkX.<br>4. Транзакции `BEGIN` / `COMMIT` / `ROLLBACK` с Snapshot Isolation. |
| **v0.8.0** | **Ruisdael** | **CBO Optimizer & Graph ML** | 1. Cost-Based Optimizer на основе статистики `ANALYZE GRAPH;`.<br>2. GPU Node2Vec и GraphSAGE внутри СУБД.<br>3. Взвешенный `shortestPath` (Dijkstra, A*).<br>4. GPU Memory Paging (UVM) для графов, превышающих VRAM. |
| **v1.0.0** | **Erasmus** | **Enterprise Security & LTS** | 1. TLS / mTLS и JWT-аутентификация.<br>2. Role-Based Access Control (RBAC).<br>3. Потоковый `InstallSnapshot` в Multi-Raft.<br>4. Долговременный LTS-релиз с гарантией стабильности API. |

---

## 🎯 Детализация ключевых направлений

### 1. Выполнение нескольких запросов через `;` (v0.5.0)
- **Лексический анализатор скриптов (`ScriptSplitter`)**: надёжное разбиение потока команд по `;` с игнорированием разделителей внутри строковых литералов и комментариев (`//`, `--`, `#`).
- **GDB Studio UI**:
  - Кнопка *Run All* и выполнение выделенного текста (*Run Selected*).
  - Пошаговый визуальный индикатор исполнения (например: `Шаг 4 из 18`).
  - Остановка при первой ошибке с подсветкой сбойной строки.
  - Сводный отчёт: суммарное время, количество затронутых строк (`rows_affected`), визуализация графа для завершающего `MATCH`.
- **Серверный API**: эндпоинт `POST /batch` для атомарного прогона пакета запросов.

### 2. GPU Offload для задач поиска и обхода графа (v0.6.0)
- **Интеграция с планировщиком**: проброс `GpuDispatcher` в `QueryExecutor`.
- **Адаптивный оператор `VarLengthExpand`**:
  - При поиске путей глубже 1 хопа (`MATCH (a)-[*1..5]->(b)`) и объёме графа выше `--gpu-threshold` (по умолчанию 10 000 рёбер) обход передаётся в ядро `parallel_bfs_step` (Apple Metal UMA / NVIDIA CUDA).
  - Волновой обход (Frontier Expansion) выполняется параллельно тысячами потоков GPU за миллисекунды.
- **Параллельная предикатная фильтрация (Columnar Scan)**:
  - Сканирование колонок свойств Apache Arrow на GPU с вычислением selection vector на скорости шины памяти (>800 ГБ/с на Mac UMA, >1 ТБ/с на CUDA).
- **Поиск подграфов и клик**:
  - Параллельное пересечение списков смежности на GPU для быстрого поиска циклических шаблонов и мотивов (`(a)->(b)->(c)->(a)`).

### 3. Нативное хранение векторных эмбеддингов и HNSW-индекс (v0.6.0)
- **Тип данных**: `VECTOR(dim)` (`f32` фиксированной размерности: 384, 768, 1536, 3072).
- **Хранение**: `FixedSizeListArray<Float32>` в Apache Arrow с нулевыми накладными расходами памяти.
- **HNSW Индекс**:
  - Метрики: `Cosine Similarity`, `Dot Product`, `Euclidean (L2)`.
  - Аппаратное ускорение расчета расстояний: SIMD (AVX2/AVX-512, NEON) и тензорные ядра GPU / Metal MPS.
- **Cypher Процедура**:
  ```sql
  CALL vector.similaritySearch('Document', 'embedding', [0.024, -0.198, ...], 10)
  YIELD node, similarity
  MATCH (node)-[:AUTHORED_BY]->(author:Person)
  RETURN node.title, author.name, similarity;
  ```

### 4. Python SDK (`gdb-py-client`) и бенчмарк (v0.7.0)
- **Скрипт бенчмарка `scripts/py_client_benchmark.py`**:
  - Сравнение пропускной способности инжеста: Arrow Flight RPC vs HTTP REST API.
  - Массовая пачечная вставка (batch insert) через Polars DataFrames.
  - Замер перцентилей задержек (P50, P95, P99) при конкурентной нагрузке.
  - Автоматическая подготовка и очистка схемы (`--keep-schema`).
- **Интеграция с ML**: Zero-Copy экспорт в `torch_geometric.data.Data` и `networkx.DiGraph`.
