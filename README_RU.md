[English](README.md) | [Русский](README_RU.md)

# GDB — Распределенная высокопроизводительная In-Memory Графовая СУБД (Альтернатива Nebula Graph)

[![Version](https://img.shields.io/badge/version-v0.5.2-blue.svg)](Cargo.toml)
[![CI & Code Coverage](https://github.com/kirill/gdb/actions/workflows/ci.yml/badge.svg)](.github/workflows/ci.yml)
[![Coverage](https://img.shields.io/badge/coverage-80.8%25-brightgreen)](docs/COVERAGE.md)
[![License](https://img.shields.io/badge/license-Apache--2.0-green.svg)](LICENSE)

Высокопроизводительный распределенный HTAP графовый движок СУБД на **Rust**, включающий in-memory топологию Compressed Sparse Row (CSR), столбчатое хранение свойств Apache Arrow, поддержку языка openCypher/GQL и DML мутаций, вторичные индексы свойств, беслидерное кольцо Multi-Raft репликации, многоуровневое хранилище с персистентностью в S3 (Parquet), распределенный MPP обмен через Apache Arrow Flight, **аппаратное GPU-ускорение Apple Metal (UMA Zero-Copy) / NVIDIA CUDA с пейджингом памяти дискретных GPU**, нативные **векторные эмбеддинги & поиск сходства**, и **Cost-Based Optimizer (CBO)**.

---

## ⚡ Ключевые возможности

1. **Экстремальная производительность In-Memory (Dual-Store & Вторичные индексы):**
   - **Chunked-CSR (Топология):** Выравнивание по 64-байтным кэш-линиям CPU, скорость обхода — **293.9 МИЛЛИОНА переходов в секунду (`hops/sec`)** на 1 ядре Apple M5.
   - **Delta MemTable (OLTP):** Параллельный lock-free буфер мутаций с MVCC Snapshot Isolation — скорость вставки **31.7 МИЛЛИОНА ребер в секунду**.
   - **Apache Arrow (Свойства):** Столбчатое хранение свойств для векторизованной SIMD-фильтрации.
   - **Вторичные индексы свойств (Secondary Indexes):** Быстрый $O(1)$ точечный поиск по свойствам сущностей (`CREATE INDEX ON :Label(prop)` / `DROP INDEX`), автоматическое обновление индекса при вставках, мутациях и удалениях.
   - **Фоновый микро-компактор:** Слияние Delta MemTable в плотный CSR без блокировки чтения.

2. **Язык запросов, Multi-Statement DML & Аналитика (openCypher / GQL / CALL algo):**
   - **Исполнение скриптов с несколькими выражениями & Batch API:** Запросы, разделенные точкой с запятой (`split_statements`), мультизапросный эндпоинт `/query`, эндпоинт `/batch`, и кнопка «Run All» в Studio UI с отслеживанием прогресса каждого шага.
   - **Cypher Relationship DML:** `MATCH (a:User), (b:User) CREATE (a)-[r:FOLLOWS]->(b)`, `MERGE (a)-[r:KNOWS]->(b)`.
   - **Шаблоны и Multi-Hop обходы:** `MATCH (a:User)-[:FOLLOWS*1..3]->(b:User) WHERE a.age > 25 RETURN b.name` с GPU-ускорением BFS.
   - **Конвейер оператора `WITH`:** Промежуточные проекции, группировки и фильтрации.
   - **Cypher DML (Мутации & Слияния):** `MATCH (u:User {name: 'Alice'}) SET u.age = 31`, `DELETE`, `DETACH DELETE`, `MERGE`.
   - **Инспекция плана выполнения (EXPLAIN):** `EXPLAIN <query>` генерирует оптимизированный физический план выполнения запроса.
   - **Пакет Nebula Enterprise Analytics & Graph ML:** `CALL algo.node2vec(...)`, `CALL algo.pageRank(...)`, `CALL algo.louvain(...)`, `CALL algo.wcc(...)`, `CALL algo.triangleCount(...)`, `CALL algo.kCore(...)`, `CALL algo.betweenness(...)`, `CALL algo.sssp(...)`, `CALL algo.similarity(...)`.

3. **Нативные векторные эмбеддинги и поиск сходства:**
   - **Тип `VECTOR(dim)`:** Столбчатое представление в Apache Arrow `FixedSizeList` и парсер векторных литералов `[1.0, 2.0, 3.0]`.
   - **Поиск векторного сходства:** `CALL vector.similaritySearch(label, property, query_vector, k, metric)` с поддержкой метрик косинусного сходства (`cosine`), скалярного произведения (`dot`) и евклидова расстояния (`l2`).
   - **Оптимизатор на основе стоимости (CBO):** `ANALYZE GRAPH;` со сбором статистики распределения степеней и кардинальности меток.

4. **Аппаратное GPU-ускорение (Metal / CUDA) и Out-Of-Core пейджинг памяти дискретных GPU:**
   - **Максимальный GPU Offload графовых операций:** Автоматический перенос на GPU для обходов переменной длины (`VarLengthExpand` wavefront BFS), вычисления векторного сходства и графовых алгоритмов.
   - **Пейджинг памяти дискретных GPU (Out-Of-Core):** Оконная потоковая передача CSR (Windowed Chunked CSR Streaming) с двойной буферизацией предотвращает ошибки Out-Of-Memory (OOM) на дискретных GPU (NVIDIA CUDA), когда объем графа превышает объем видеопамяти VRAM (`--gpu-max-vram-mb`). Легковесный массив смещений `offsets` удерживается в VRAM, а порции целевых вершин `targets` асинхронно передаются по шине PCIe через DMA.
   - **Apple Silicon (M-серия):** Архитектура единой памяти **Unified Memory Architecture (UMA)** позволяет графическому процессору читать топологию графа из RAM **напрямую с нулевой стоимостью копирования (Zero-Copy)**.
   - **Linux NVIDIA (CUDA):** Нативный CudaComputeBackend в `gdb-gpu` для параллельных вычислений на серверных GPU.
   - **CPU SIMD Fallback:** При выключенном GPU или отсутствии графического процессора автоматически используется векторизованный параллельный бэкенд на Rayon.

5. **GDB Studio v0.5.2 (Интерактивный Web Workspace):**
   - **Выполнение скриптов «Run All»:** Индикатор прогресса шагов (`Step: X / Y`), пакетное исполнение и подсветка ошибок.
   - **Инспектор векторов:** Аккуратное моноширинное форматирование многомерных векторов и скоров сходства.
   - **Чистый каталог по умолчанию & Interactive Schema Manager:** Просмотр, создание, изменение и удаление сущностей визуально.
   - **Физическая визуализация & Execution Plan DAG:** 60 FPS Canvas & 3D Unity WebGL, визуализатор плана выполнения запроса, спарклайны телеметрии.

6. **Официальный Python SDK & Arrow Flight (`gdb-client`):**
   - **Автономный пакет `gdb-client`:** Официальный клиент [**`gdb-client`**](https://github.com/1orgar/gdb-py-client) на [PyPI](https://pypi.org/project/gdb-client/) (`pip install gdb-client`) с экспортом в Polars DataFrame, PyArrow, NetworkX и параллельной загрузкой через Arrow Flight.
   - **Автоматизированный бенчмарк:** `scripts/py_client_benchmark.py` с автоматическим созданием и очисткой схемы (`--keep-schema`).
   - **Разделение портов:** Межсервисный MPP shuffle (`--port 8848+`) изолирован от клиентского порта (`--client-flight-port 8860+`).

7. **Горизонтальное масштабирование & Персистентность в S3:**
   - Беслидерное кольцо с настраиваемым фактором репликации (`--replication-factor 1..N`) и режимом (`--sync` / `--async`).
   - Асинхронный сброс снапшотов партиций в **S3 / MinIO** в формате **Apache Parquet (ZSTD)**.

---

## 🚀 Запуск 3-узлового кластера из бинарников (ARM Mac + GPU)

В каталоге [`bin/`](bin/) находятся готовые оптимизированные бинарники под архитектуру ARM Mac (`Mach-O 64-bit arm64`):
- `bin/gdb-server` — сервер ноды (Arrow Flight + HTTP REST + Multi-Raft + Metal GPU).
- `bin/gdb-cli` — интерактивная консоль запросов и пакетный исполнитель.
- `bin/gdb-studio` — веб-интерфейс и аналитический воркспейс.

### Вариант 1: Быстрый запуск кластера одним скриптом (Рекомендуется)

Скрипт запуска автоматически находит бинарники (в `bin/` или `target/release/`) и поддерживает параметры репликации и GPU:

```bash
# 1. Полная синхронная репликация (3 ноды, RF=3, SYNC — по умолчанию):
./scripts/start_cluster.sh --nodes 3 --rf 3 --sync

# 2. Выбор произвольного числа нод (например, 5 нод или одиночная нода):
./scripts/start_cluster.sh --nodes 5 --rf 3 --sync
./scripts/start_cluster.sh --nodes 1

# 3. Включение GPU-ускорения (Apple Metal / NVIDIA CUDA):
./scripts/start_cluster.sh --nodes 3 --rf 3 --sync --enable-gpu true --gpu-offload-threshold 10000

# 4. Асинхронная репликация (RF=3, ASYNC — максимальный TPS):
./scripts/start_cluster.sh --nodes 3 --rf 3 --async
```

Вывод:
```
============================================================
       Starting 3-Node GDB Cluster (Leaderless Ring)        
       Replication Factor: RF=3 | Mode: SYNC       
============================================================
[+] Peer 1 started (PID 61877): Flight :8848 | HTTP :8847 | RF: 3 | Mode: sync | Role: Peer
[+] Peer 2 started (PID 61878): Flight :8849 | HTTP :8846 | RF: 3 | Mode: sync | Role: Peer
[+] Peer 3 started (PID 61879): Flight :8850 | HTTP :8845 | RF: 3 | Mode: sync | Role: Peer

[✓] 3-node cluster is healthy and ready for queries!
    CLI connect:   ./bin/gdb-cli
    Web Studio UI: ./scripts/start_studio.sh (http://localhost:3000)
    HTTP endpoint: http://localhost:8847/query
    Logs:          tail -f logs/node*.log
    Stop cluster:  ./scripts/stop_cluster.sh
```

Остановка кластера:
```bash
./scripts/stop_cluster.sh
```

---

### Вариант 2: Ручной запуск 3 нод в отдельных терминалах

#### Терминал 1: Peer 1 (:8847 / :8848)
```bash
./bin/gdb-server \
  --node-id 1 \
  --partitions 8 \
  --port 8848 \
  --http-port 8847 \
  --wal-dir ./data/node1/wal \
  --peers http://127.0.0.1:8846,http://127.0.0.1:8845 \
  --replication-factor 3 \
  --replication-mode sync \
  --enable-gpu false
```

#### Терминал 2: Peer 2 (:8846 / :8849)
```bash
./bin/gdb-server \
  --node-id 2 \
  --partitions 8 \
  --port 8849 \
  --http-port 8846 \
  --wal-dir ./data/node2/wal \
  --peers http://127.0.0.1:8847,http://127.0.0.1:8845 \
  --replication-factor 3 \
  --replication-mode sync \
  --enable-gpu false
```

#### Терминал 3: Peer 3 (:8845 / :8850)
```bash
./bin/gdb-server \
  --node-id 3 \
  --partitions 8 \
  --port 8850 \
  --http-port 8845 \
  --wal-dir ./data/node3/wal \
  --peers http://127.0.0.1:8847,http://127.0.0.1:8846 \
  --replication-factor 3 \
  --replication-mode sync \
  --enable-gpu false
```

---

## 💻 Запуск 3-узлового кластера на архитектуре AMD64 (x86_64)

В отдельном каталоге [`bin/amd64/`](bin/amd64/) собраны релизные бинарники под архитектуру AMD64 (`x86_64`):
- `bin/amd64/gdb-server` — сервер ноды под x86_64.
- `bin/amd64/gdb-cli` — интерактивная консоль под x86_64.
- `bin/amd64/gdb-studio` — Web Studio под x86_64.

### Быстрый запуск AMD64 кластера:

```bash
./scripts/start_cluster_amd64.sh --rf 3 --sync
```

Остановка AMD64 кластера:
```bash
./scripts/stop_cluster_amd64.sh
```

---

## 🌐 GDB Studio — Web UI & Интерактивный Графический Workspace

**GDB Studio** — это графический веб-интерфейс СУБД GDB, скомпилированный в виде автономного Rust-бинаря (`bin/gdb-studio`):
- **Не требует Node.js, npm или внешних серверов** — вся статика и JS-движки встроены в бинарник.
- **Вкладка `🕸️ Graph View`:** Интерактивная 60 FPS Canvas визуализация, инспектор сущностей, экспорт в PNG/SVG/CSV/JSON.
- **Вкладка `📊 Table View`:** Табличный просмотр аналитических выборок.
- **Вкладка `🔍 Plan / Explain`:** Графический визуализатор физического DAG-плана исполнения запросов.
- **Вкладка `🌐 Cluster Ring`:** Топология кольца, роли нод, диапазоны токенов и быстрое переключение между узлами.
- **Вкладка `⚡ Storage & Resources`:** Мониторинг RAM, графа, статуса S3 и графики Telemetry Sparklines (QPS, Latency, RAM).
- **Вкладка `🎮 3D Unity View`:** 3D WebGL визуализатор графа в пространстве.

### Быстрый запуск Studio:
```bash
./scripts/start_studio.sh
# Открыть в браузере: http://localhost:3000
```

Остановка Studio:
```bash
./scripts/stop_studio.sh
```

---

## 🎮 Подключение и выполнение запросов через CLI

```bash
./bin/gdb-cli
```

### Примеры запросов:

```sql
-- 1. Определение схемы и вторичных индексов
CREATE VERTEX User (name STRING, age INT64);
CREATE EDGE FOLLOWS ();
CREATE INDEX ON :User(name);

-- 2. Вставка данных (одиночная и пакетная)
INSERT VERTEX User (id, name, age) VALUES (1, 'Alice', 30);
INSERT VERTEX User (id, name, age) VALUES (2, 'Bob', 25), (3, 'Charlie', 35);
INSERT EDGE FOLLOWS VALUES (1, 2), (2, 3), (3, 1);

-- 3. Мутации и Upsert (Cypher DML)
MATCH (u:User {name: 'Alice'}) SET u.age = 31;
MERGE (u:User {name: 'Dave', age: 28});

-- 4. Траверс графа, агрегации и пагинация
MATCH (a:User)-[:FOLLOWS]->(b:User)
RETURN a.name, COUNT(b) AS followers
ORDER BY followers DESC;

-- 5. Инспекция плана исполнения (EXPLAIN)
EXPLAIN MATCH (a:User)-[:FOLLOWS]->(b:User) WHERE a.name = 'Alice' RETURN b.name;

-- 6. Корпоративная графовая аналитика (GPU/SIMD)
CALL algo.pageRank({damping: 0.85, max_iter: 20}) YIELD vertex_id, score;
CALL algo.louvain({max_iter: 10}) YIELD vertex_id, community_id;
CALL algo.wcc() YIELD vertex_id, component_id;
CALL algo.triangleCount() YIELD vertex_id, triangles;

-- 7. Интроспекция кластера
SHOW CLUSTER;
SHOW RESOURCES;
SHOW GPU;
```

---

## 🧪 Тестирование и покрытие кода (Code Coverage)

В GDB реализован полный набор юнит-, интеграционных и E2E тестов с автоматическим отслеживанием покрытия:

```bash
# Запуск полного набора тестов рабочего пространства
cargo test --workspace

# Генерация отчета о покрытии кода (cargo-llvm-cov)
./scripts/coverage.sh

# Открытие интерактивного HTML-отчета
cargo llvm-cov --workspace --html --open
```

Подробное описание тест-сьютов и покрытие по компонентам доступно в **[docs/COVERAGE.md](docs/COVERAGE.md)**.

---

## 📖 Подробная документация

- 👉 **[Руководство по тестированию и бенчмаркам (TESTING_AND_BENCHMARKS_RU.md)](docs/TESTING_AND_BENCHMARKS_RU.md)**
- 👉 **[Руководство по серверу и запросам (SERVER_AND_QUERY_GUIDE_RU.md)](docs/SERVER_AND_QUERY_GUIDE_RU.md)**
- 👉 **[Отчет о тестировании и покрытии кода (COVERAGE.md)](docs/COVERAGE.md)**
- 👉 **[Руководство по эксплуатации и загрузке данных (OPERATIONS_GUIDE_RU.md)](docs/OPERATIONS_GUIDE_RU.md)**

---

## Лицензия
Apache-2.0
