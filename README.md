# GDB — Distributed High-Performance In-Memory Graph Database (Nebula Alternative)

A next-generation, high-performance distributed HTAP graph database engine built with **Rust**, featuring in-memory Compressed Sparse Row (CSR) topology, Apache Arrow columnar properties, openCypher/GQL query support, Multi-Raft replication, S3-backed tiered persistence, MPP distributed exchange over Arrow Flight, and **Apple Metal (UMA Zero-Copy) / NVIDIA CUDA GPU hardware acceleration**.

---

## ⚡ Ключевые возможности

1. **Экстремальная производительность In-Memory (Dual-Store):**
   - **Chunked-CSR (Топология):** Выравнивание по 64-байтным кэш-линиям CPU, скорость обхода — **293.9 МИЛЛИОНА переходов в секунду (`hops/sec`)** на 1 ядре Apple M5.
   - **Delta MemTable (OLTP):** Параллельный lock-free буфер мутаций с MVCC Snapshot Isolation — скорость вставки **31.7 МИЛЛИОНА ребер в секунду**.
   - **Apache Arrow (Свойства):** Столбчатое хранение свойств для векторизованной SIMD-фильтрации.
   - **Фоновый микро-компактор:** Слияние Delta MemTable в плотный CSR без блокировки чтения.

2. **Язык запросов & Аналитика (openCypher / GQL / CALL algo):**
   - Стандартные сопоставления шаблонов: `MATCH (a:User)-[:FOLLOWS]->(b:User) WHERE a.age > 25 RETURN b.name`.
   - Полный пакет **Nebula Enterprise Analytics**: `CALL algo.pageRank(...)`, `CALL algo.louvain(...)`, `CALL algo.wcc(...)`, `CALL algo.triangleCount(...)`, `CALL algo.kCore(...)`, `CALL algo.betweenness(...)`, `CALL algo.sssp(...)`, `CALL algo.similarity(...)`.

3. **Горизонтальное масштабирование и Беслидерное кольцо (Leaderless Hash Ring):**
   - Равноправные узлы (Symmetric Peers) в стиле Dynamo/Cassandra без единой точки отказа (SPOF).
   - Настраиваемый фактор репликации (`--replication-factor 1..N`) и режим (`--sync` / `--async`).
   - При RF=1 кластер работает как чистый MPP с распределенным шардированием (1D Edge Cut) без избыточности.
   - Независимые Raft-партиции с локальным журналом `gdb-wal` (CRC32).

4. **Аппаратное GPU-ускорение (Metal на Mac / CUDA на Linux):**
   - **Apple Silicon (M-серия):** Архитектура единой памяти **Unified Memory Architecture (UMA)** позволяет графическому процессору читать топологию графа из RAM **напрямую с нулевой стоимостью копирования (Zero-Copy)**.
   - **Linux NVIDIA (CUDA):** Нативный CudaComputeBackend в `gdb-gpu` для параллельных вычислений BFS и PageRank SpMV на серверных GPU (A100, H100, RTX).
   - Адаптивный диспетчер: автоматический офлоад тяжелых обходов и аналитики на GPU-ядра Metal Compute или CUDA.

5. **Выделенный транспорт Arrow Flight & Клиентская библиотека Python:**
   - **Разделение портов:** Межсервисный MPP shuffle (`--port 8848+`) изолирован от внешнего высокоскоростного порта загрузки и запросов (`--client-flight-port 8860+`).
   - **Python Клиент (`gdb-py-client`):** Параллельный scatter-ingest на основе **Polars** и Arrow Flight Streaming `do_put` напрямую в партиции целевых нод.

6. **Персистентность в S3 (Tiered Storage):**
   - Асинхронный сброс снапшотов партиций в **S3 / MinIO** в сжатом формате **Apache Parquet (ZSTD)**.
   - Быстрое восстановление при сбое: загрузка Parquet из S3 + replay последних записей Raft WAL.

---

## 🚀 Запуск 3-узлового кластера из скомпилированных бинарников (ARM Mac + GPU)

В каталоге [`bin/`](bin/) находятся готовые оптимизированные бинарники под архитектуру ARM Mac (`Mach-O 64-bit arm64`):
- `bin/gdb-server` — сервер ноды (Arrow Flight + HTTP REST + Multi-Raft + Metal GPU).
- `bin/gdb-cli` — интерактивная консоль запросов и пакетный исполнитель.

### Вариант 1: Быстрый запуск кластера одним скриптом (Рекомендуется)

В проекте подготовлен скрипт автоматического запуска 3 узлов с разделением портов, WAL-каталогов и включением GPU Metal:

```bash
# 1. Полная синхронная репликация (RF=3, SYNC — по умолчанию):
./scripts/start_cluster.sh --rf 3 --sync

# 2. Асинхронная репликация (RF=3, ASYNC — максимальный TPS):
./scripts/start_cluster.sh --rf 3 --async

# 3. Чистое распределенное шардирование без репликации (RF=1, SYNC):
./scripts/start_cluster.sh --rf 1 --sync
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

Если вы хотите запустить ноды вручную и наблюдать за логами каждой ноды в реальном времени:

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
  --replication-mode sync
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
  --replication-mode sync
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
  --replication-mode sync
```

---

## 💻 Запуск 3-узлового кластера на архитектуре AMD64 (x86_64)

В отдельном каталоге [`bin/amd64/`](bin/amd64/) собраны релизные бинарники под архитектуру AMD64 (`x86_64`):
- `bin/amd64/gdb-server` — сервер ноды под x86_64 (Arrow Flight + HTTP REST + Multi-Raft).
- `bin/amd64/gdb-cli` — интерактивная консоль запросов и пакетный исполнитель под x86_64.

### Вариант 1: Быстрый запуск AMD64 кластера одним скриптом

```bash
# Запуск 3-узлового кластера под AMD64
./scripts/start_cluster_amd64.sh
```

Вывод:
```
============================================================
       Starting 3-Node GDB Cluster (AMD64 / x86_64)         
============================================================
[+] Node 1 started (PID 51201): Flight :8848 | HTTP :8847 | Arch: AMD64
[+] Node 2 started (PID 51202): Flight :8849 | HTTP :8846 | Arch: AMD64
[+] Node 3 started (PID 51203): Flight :8850 | HTTP :8845 | Arch: AMD64

[✓] 3-node AMD64 cluster is healthy and ready for queries!
    CLI connect:   ./bin/amd64/gdb-cli
    HTTP endpoint: http://localhost:8847/query
    Logs:          tail -f logs/amd64/node*.log
    Stop cluster:  ./scripts/stop_cluster_amd64.sh
```

Остановка AMD64 кластера:
```bash
./scripts/stop_cluster_amd64.sh
```

### Вариант 2: Ручной запуск 3 нод на AMD64

```bash
# Peer 1 (:8847 / :8848)
./bin/amd64/gdb-server --node-id 1 --partitions 8 --port 8848 --http-port 8847 --wal-dir ./data/amd64/node1/wal --peers http://127.0.0.1:8846,http://127.0.0.1:8845 --replication-factor 3 --replication-mode sync

# Peer 2 (:8846 / :8849)
./bin/amd64/gdb-server --node-id 2 --partitions 8 --port 8849 --http-port 8846 --wal-dir ./data/amd64/node2/wal --peers http://127.0.0.1:8847,http://127.0.0.1:8845 --replication-factor 3 --replication-mode sync

# Peer 3 (:8845 / :8850)
./bin/amd64/gdb-server --node-id 3 --partitions 8 --port 8850 --http-port 8845 --wal-dir ./data/amd64/node3/wal --peers http://127.0.0.1:8847,http://127.0.0.1:8846 --replication-factor 3 --replication-mode sync
```

> [!NOTE]
> На машинах Apple Silicon запуск x86_64 бинарников выполняется через транслятор Rosetta 2 (`softwareupdate --install-rosetta --agree-to-license`). Скрипт `start_cluster_amd64.sh` автоматически определяет окружение и использует `arch -x86_64`. На нативных x86_64 серверах (Intel/AMD) бинарники запускаются нативно.


---

## 🌐 GDB Studio — Web UI & Интерактивный Графический Workspace

**GDB Studio** — это графический веб-интерфейс и аналитический воркспейс СУБД GDB, скомпилированный в виде автономного Rust-бинаря (`bin/gdb-studio` и `bin/amd64/gdb-studio`). Все веб-ресурсы (HTML5, стили, высокопроизводительный Canvas движок физики графа) встроены внутрь бинарника: для работы **не требуются Node.js, npm или внешние веб-серверы**.

### 🌟 Ключевые возможности:
1. **Интерактивный редактор (openCypher / GQL):**
   - Удобный редактор с историей запросов, каталогом схемы (Vertex Tags / Edge Types) и быстрыми шаблонами (1-Hop/2-Hop обходы, PageRank, Louvain, WCC, SSSP, DDL).
   - Горячая клавиша быстрого запуска: `Cmd+Enter` или `Ctrl+Enter`.
2. **Физическая визуализация графа (Graph View 60 FPS & Unity 3D WebGL):**
   - Симуляция силовых полей (Force-Directed Graph) на HTML5 Canvas с поддержкой Retina-дисплеев.
   - **Unity 3D WebGL Visualizer:** Встроенная трехмерная визуализация графа в пространстве (WebGL Bridge) с физикой отталкивания и интерактивным выбором узлов.
   - **Защита от зависания UI (Issue #3):** Автоматическое переключение в Table View при аналитических запросах (`CALL algo.*`), ограничение симуляции до 250 узлов и остановка анимационного цикла при скрытой вкладке для нулевой загрузки CPU.
   - Зум (колесико мыши), свободное панорамирование (Pan), перетаскивание узлов (Drag-and-Drop).
   - Направленные стрелки связей с подписями типов отношений (`[:FOLLOWS]`, `[:KNOWS]`).
   - Цветовая палитра по типам сущностей и сообществам алгоритмов.
3. **Инспектор сущностей (Inspector):**
   - Клик на любой узел или связь отображает карточку с ID, меткой, свойствами и списком смежных ребер (двунаправленный мост Canvas/Unity к Inspector).
4. **Табличное представление (Table View):**
   - Сортируемая сетка данных с фиксацией заголовков для аналитических запросов (`CALL algo.pageRank()`, `CALL algo.louvain()`).
5. **Мониторинг кластера:**
   - Отображение статуса нод (Peer 1 :8847, Peer 2 :8846, Peer 3 :8845) и возможность динамической смены URL подключения.

### 🚀 Быстрый запуск GDB Studio:

```bash
# 1. Запустить Studio (по умолчанию подключается к http://localhost:8847)
./scripts/start_studio.sh

# 2. Открыть в браузере:
# http://localhost:3000

# 3. Остановка Studio:
./scripts/stop_studio.sh
```

### Запуск бинарника напрямую с параметрами:
```bash
# ARM Mac (Apple Silicon)
./bin/gdb-studio --port 3000 --cluster-url http://localhost:8847

# AMD64 (x86_64)
./bin/amd64/gdb-studio --port 3000 --cluster-url http://localhost:8847
```

---

## 🎮 Подключение и выполнение запросов через CLI

CLI (`bin/gdb-cli` и `bin/amd64/gdb-cli`) автоматически проверяет соединение с кластером при старте:
```bash
./bin/gdb-cli
```
Вывод приветствия и статус проверки здоровья ноды:
```
   ______  ____  ____ 
  / ____/ / __ \/ __ )
 / / __  / / / / __  |
/ /_/ / / /_/ / /_/ / 
\____(_)_____/_____/  Interactive Cypher Shell v0.3.0

[✓] Connected to GDB Node at http://localhost:8847 (Latency: 0.8ms)
    Type 'help' or '\?' for help. Press Ctrl+D to exit.
```

### Команды интроспекции кластера, ресурсов и GPU:
В интерактивной консоли доступны специализированные управляющие команды:

| Команда | Описание |
| :--- | :--- |
| `SHOW CLUSTER` | Таблица всех равноправных узлов кольца: Node ID, роль (Peer), порты Flight/HTTP, фактор репликации (RF) и режим (sync/async). |
| `SHOW RESOURCES` | Метрики потребления: объем памяти (RSS), аптайм, QPS, кол-во вершин и ребер (CSR vs MemTable), компактизации. |
| `SHOW GPU` | Статус графического ускорителя: активный бэкенд (Apple Metal / CUDA / CPU SIMD), UMA Zero-Copy, порог офлоада. |
| `:connect <url>` | Динамическое переключение текущей сессии CLI на любой другой узел кольца (например, `:connect http://localhost:8846`). |

#### Примеры вывода команд:
```sql
gdb> SHOW CLUSTER;
+---------+------+-------------+-----------+----+------+
| Node ID | Role | Flight Port | HTTP Port | RF | Mode |
+---------+------+-------------+-----------+----+------+
| 1       | Peer | 8848        | 8847      | 3  | sync |
| 2       | Peer | 8849        | 8846      | 3  | sync |
| 3       | Peer | 8850        | 8845      | 3  | sync |
+---------+------+-------------+-----------+----+------+

gdb> SHOW RESOURCES;
+------------------+---------+
| Metric           | Value   |
+------------------+---------+
| Uptime (sec)     | 420     |
| Total Queries    | 15280   |
| Current QPS      | 7455.6  |
| Memory RSS       | 48.2 MB |
| Total Vertices   | 10000   |
| Total Edges      | 100000  |
| CSR Edges        | 100000  |
| Delta Edges      | 0       |
| Total Compactions| 1       |
+------------------+---------+

gdb> SHOW GPU;
+------------------+------------------------------------+
| Parameter        | Value                              |
+------------------+------------------------------------+
| Available        | true                               |
| Backend          | Apple Metal Compute (UMA Zero-Copy)|
| Device           | Apple M5                           |
| Dispatch Status  | Active                             |
| Offload Threshold| 10000 edges                        |
+------------------+------------------------------------+
```

### Примеры запросов данных:
```sql
-- 1. Создание схемы
CREATE VERTEX User (name STRING, age INT64);
CREATE EDGE FOLLOWS ();

-- 2. Вставка данных (одиночная и пакетная Cypher Bulk Insert)
INSERT VERTEX User (id, name, age) VALUES (1, 'Alice', 30);
-- Пакетная вставка вершин (Bulk Insert):
INSERT VERTEX User (id, name, age) VALUES (2, 'Bob', 25), (3, 'Charlie', 35), (4, 'Dave', 28);

-- Одиночная вставка ребер:
INSERT EDGE FOLLOWS FROM 1 TO 2;
-- Пакетная вставка ребер (Bulk Insert):
INSERT EDGE FOLLOWS VALUES (2, 3), (3, 1), (3, 4);

-- 3. Принудительная компактизация в Chunked-CSR
compact;

-- 4. Обход графа (k-hop Cypher & Star Cast Multi-Hop)
-- Фиксированные переходы:
MATCH (a:User)-[:FOLLOWS]->(b:User)-[:FOLLOWS]->(c:User)
RETURN a.name, b.name, c.name;

-- Переменная длина пути (Multi-hop Variable-Length traversal 1..3 hops):
MATCH (a:User)-[:FOLLOWS*1..3]->(b:User)
WHERE a.id = 1
RETURN a.name, b.name;
```

### Запуск запросов без входа в REPL (скриптовый режим):
```bash
# Одиночный запрос
./bin/gdb-cli -e "CALL algo.pageRank() YIELD vertex_id, score"

# Пакетное исполнение из файла
./bin/gdb-cli -f data/my_script.gdb
```

---

## 🧠 Использование ресурсов GPU (Metal Compute на Apple Silicon)

Сервер `gdb-server` при запуске автоматически определяет платформу:
```
[+] Hardware Acceleration: Apple Metal Compute (UMA Zero-Copy)
```

### Как работает GPU-ускорение:
1. **Zero-Copy архитектура (UMA):** На процессорах Apple Silicon (M1/M2/M3/M4/M5) оперативная память CPU и GPU физически едина. Топология графа `ChunkedCsr` мапится в буферы Metal Shading Language **без пересылки через шину PCIe**.
2. **Адаптивный диспетчер:**
   - Небольшие выборки ($< 10,000$ ребер) обрабатываются на быстрых CPU-ядрах.
   - Массовые аналитические расчеты ($> 10,000$ ребер) автоматически перенаправляются на графические ядра Metal.

### Запуск алгоритмов с поддержкой GPU-ускорения:
```sql
-- PageRank (20 итераций на 1 млн ребер занимает всего 6 миллисекунд!)
CALL algo.pageRank({damping: 0.85, max_iter: 20}) YIELD vertex_id, score;

-- Louvain Community Detection (кластеризация сообществ)
CALL algo.louvain({max_iter: 10}) YIELD vertex_id, community_id;

-- Weakly Connected Components (WCC)
CALL algo.wcc() YIELD vertex_id, component_id;

-- Triangle Count & Local Clustering Coefficient (LCC)
CALL algo.triangleCount() YIELD vertex_id, triangles;

-- SSSP (Кратчайшие расстояния от вершины 1 ко всем остальным)
CALL algo.sssp({source: 1}) YIELD vertex_id, distance;

-- Метрики сходства вершин
CALL algo.similarity({node1: 1, node2: 2}) YIELD jaccard, common_neighbors;
```

---

## 📊 Prometheus Метрики (`GET /metrics`)

Каждый узел `gdb-server` содержит встроенный легковесный экспортер метрик OpenMetrics / Prometheus, доступный по адресу `http://<host>:<http-port>/metrics`.

### Основные собираемые метрики:
- `gdb_uptime_seconds` — время непрерывной работы узла в секундах.
- `gdb_queries_total` — общее число обработанных запросов.
- `gdb_query_duration_seconds` — квантили времени выполнения (p50, p90, p99).
- `gdb_graph_vertices_total` — общее число вершин в графе.
- `gdb_graph_edges_total` — общее число ребер в графе.
- `gdb_graph_csr_edges` — количество ребер в уплотненном Chunked-CSR.
- `gdb_graph_memtable_edges` — количество «горячих» ребер в буфере Delta MemTable.
- `gdb_graph_compactions_total` — количество выполненных микро-компактизаций.
- `gdb_raft_term` и `gdb_raft_is_leader` — текущий терм и статус лидерства в Raft-группе.
- `gdb_gpu_available` — флаг доступности GPU Metal / CUDA.
- `gdb_memory_resident_bytes` — объем используемой оперативной памяти (RSS).

### Пример запроса метрик:
```bash
curl -s http://localhost:8847/metrics
```
Пример ответа:
```prometheus
# HELP gdb_uptime_seconds Uptime of the GDB server node in seconds
# TYPE gdb_uptime_seconds counter
gdb_uptime_seconds{node_id="1"} 320

# HELP gdb_queries_total Total queries executed on this node
# TYPE gdb_queries_total counter
gdb_queries_total{node_id="1"} 15280

# HELP gdb_graph_vertices_total Total number of active vertices
# TYPE gdb_graph_vertices_total gauge
gdb_graph_vertices_total{node_id="1"} 10000

# HELP gdb_graph_edges_total Total number of edges across CSR and Delta MemTable
# TYPE gdb_graph_edges_total gauge
gdb_graph_edges_total{node_id="1"} 100000
gdb_graph_csr_edges{node_id="1"} 100000
gdb_graph_memtable_edges{node_id="1"} 0

# HELP gdb_cluster_replication_factor Configured replication factor on hash ring
# TYPE gdb_cluster_replication_factor gauge
gdb_cluster_replication_factor 3

# HELP gdb_cluster_is_sync_replication Whether replication is synchronous (1) or asynchronous (0)
# TYPE gdb_cluster_is_sync_replication gauge
gdb_cluster_is_sync_replication 1

# HELP gdb_gpu_available Whether GPU acceleration is active and available
# TYPE gdb_gpu_available gauge
gdb_gpu_available{node_id="1",backend="metal"} 1
```

---

## ⚙️ Полный справочник параметров запуска (CLI & Config Reference)

### 1. Параметры запуска серверного узла (`gdb-server`)

```bash
gdb-server [OPTIONS]
```

| Флаг CLI | Короткий | Переменная окружения | Тип | По умолчанию | Описание и назначение |
| :--- | :---: | :--- | :---: | :---: | :--- |
| `--node-id` | `-n` | — | `u64` | `1` | Уникальный числовой ID узла в кольце кластера. Первичный токен: $u \pmod N$. |
| `--partitions` | — | — | `u32` | `4` | Количество независимых Multi-Raft групп и локальных партиций хранилища. |
| `--port` | `-p` | — | `u16` | `8848` | Сетевой порт **Internal Apache Arrow Flight gRPC** (межсервисный обмен нод, MPP Shuffle). |
| `--client-flight-port` | — | `GDB_CLIENT_FLIGHT_PORT` | `u16` | `8860` | Сетевой порт **External Client Flight gRPC** (сверхбыстрая пакетная загрузка Polars/Arrow `do_put` и быстрые запросы `do_get`). |
| `--http-port` | — | — | `u16` | `8847` | Сетевой порт **HTTP REST API** (запросы `/query`, репликация `/replicate`, метрики `/metrics`, health-check `/health`). |
| `--wal-dir` | — | — | `path` | `./data/wal` | Каталог журнала упреждающей записи Write-Ahead Log с верификацией CRC32. |
| `--peers` | — | — | `string` | *(пусто)* | Список HTTP REST адресов других участников кольца через запятую. |
| `--replication-factor` | `-r` | `GDB_REPLICATION_FACTOR` | `u32` | `3` | **Фактор репликации кольца (RF):**<br>• `1` — чистое шардирование без дублирования (режим MPP, $\sum\text{RAM}$).<br>• `k` — частичная репликация на $k$ последовательных узлов кольца.<br>• `N` — полное зеркалирование (100% данных на всех узлах). |
| `--replication-mode` | — | `GDB_REPLICATION_MODE` | `string` | `sync` | **Режим репликации:**<br>• `sync` — синхронный: координатор ожидает подтверждения от всех реплик перед ответом клиенту.<br>• `async` — асинхронный: координатор моментально отвечает клиенту, передавая мутацию в фоновых задачах. |
| `--s3-bucket` | — | `AWS_BUCKET` | `string` | *(пусто)* | Имя бакета AWS S3 / MinIO для Tiered Storage и команды `snapshot;`. |
| `--s3-endpoint` | — | `AWS_ENDPOINT` | `string` | *(пусто)* | URL S3-совместимого сервиса (например, `http://localhost:9000`). |
| `--s3-region` | — | `AWS_REGION` | `string` | `us-east-1` | Регион AWS S3 (например, `eu-central-1`, `us-east-1`). |
| *(credentials)* | — | `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY` | `string` | *(пусто)* | Ключи доступа для аутентификации в S3/MinIO. |
| *(logging)* | — | `RUST_LOG` | `string` | `info` | Уровень детализации логирования tracing (`error`, `warn`, `info`, `debug`, `trace`). |
| `--cluster-mode` | — | — | `string` | `ring` | Алиас совместимости (`ring`, `replication`, `sharding`). |

### 2. Параметры скриптов автоматического запуска (`start_cluster.sh` и `start_cluster_amd64.sh`)

| Флаг скрипта | Алиасы | Пример использования | Описание |
| :--- | :--- | :--- | :--- |
| `--rf <N>` | `-r <N>`, `--replication-factor <N>` | `./scripts/start_cluster.sh --rf 1` | Устанавливает фактор репликации кольца (`1`, `2`, `3`). |
| `--sync` | `sync`, `--replication-mode sync` | `./scripts/start_cluster.sh --sync` | Строгая синхронная репликация с немедленной согласованностью. |
| `--async` | `async`, `--replication-mode async` | `./scripts/start_cluster.sh --async` | Фоновая асинхронная репликация для максимального TPS. |
| `--sharding` | `--sharded`, `sharding` | `./scripts/start_cluster.sh --sharding` | Алиас для `--rf 1 --sync` (чистый распределенный MPP кластер). |
| `--replication` | `--replicated`, `replication` | `./scripts/start_cluster.sh --replication` | Алиас для `--rf 3 --sync` (полная синхронная репликация). |

---

## 🔄 Беслидерная кольцевая репликация (Leaderless Hash Ring)

В GDB реализована симметричная архитектура узлов в стиле **Amazon Dynamo / Apache Cassandra**:
1. **Любая нода — Координатор:** Клиент может отправлять DDL/DML и запросы на любой узел кольца.
2. **Хэш-кольцо (Hash Ring):** Ключи вершин и ребер распределяются по кольцу по формуле $\text{Primary Node} = u \pmod N$. При $\text{RF} > 1$ запись реплицируется на следующие $\text{RF} - 1$ узлов по часовой стрелке.
3. **Режимы согласованности:**
   - **`--sync`**: Координатор ждет параллельных подтверждений от всех целевых реплик перед завершением транзакции.
   - **`--async`**: Запись немедленно фиксируется локально, а репликация выполняется в фоновых задачах `tokio::spawn`.
4. **Режим чистого MPP ($\text{RF} = 1$):** Кластер работает без избыточного дублирования, максимально эффективно масштабируя суммарный объем RAM.

---

## 📖 Подробная документация (Architecture & Query Reference)

Полное руководство по развертыванию, шардингу, GPU, персистентности в S3 и синтаксису запросов вынесено в отдельный документ:

👉 **[Руководство по запуску, конфигурированию сервера и справочник запросов](docs/SERVER_AND_QUERY_GUIDE.md)**

### Содержание руководства:
1. **Часть 1: Запуск и конфигурирование сервера:**
   - 1.1 Параметры командной строки `gdb-server`.
   - 1.2 Горизонтальное шардирование (1D Edge Cut, Source-Colocation, Multi-Raft).
   - 1.3 Аппаратное ускорение на GPU (Metal UMA Zero-Copy / CUDA, адаптивный диспетчер).
   - 1.4 Tiered Storage: Персистентность в S3 / MinIO (Parquet ZSTD + WAL Replay).
   - 1.5 Мониторинг через Prometheus (`/metrics`) и структура метрик.
2. **Часть 2: Примеры запросов и справочник по языку:**
   - 2.1 Определение схемы DDL (`CREATE VERTEX`, `CREATE EDGE`).
   - 2.2 Манипуляция данными DML (`INSERT VERTEX`, `INSERT EDGE`).
   - 2.3 openCypher / GQL запросы (1-Hop, 2-Hop, агрегации, фильтрация).
   - 2.4 Встроенная графовая аналитика (PageRank, Louvain, WCC, Triangle Count, SSSP, Similarity).
   - 2.5 Интроспекция кластера (`SHOW CLUSTER`, `SHOW RESOURCES`, `SHOW GPU`).

---

## 🐍 Тестовые Python скрипты и Бенчмарки

В каталоге [`scripts/`](scripts/) находятся утилиты, работающие на **чистом Python 3 без сторонних зависимостей** (только стандартная библиотека `urllib`, `json`, `threading`, `math`):

### 1. Наполнение базы тестовыми данными (`data_loader.py`):
```bash
# Залить 10,000 вершин и 100,000 ребер в работающий кластер по HTTP:
python3 scripts/data_loader.py --vertices 10000 --edges 100000

# Сгенерировать большой датасет в файл для пакетной загрузки:
python3 scripts/data_loader.py --vertices 50000 --edges 500000 --file data/graph_500k.gdb
./bin/gdb-cli -f data/graph_500k.gdb
```

### 2. Запуск стресс-тестов и бенчмарков (`benchmark_suite.py`):
```bash
python3 scripts/benchmark_suite.py --samples 500 --concurrency 4
```

### 3. Сквозная проверка кольцевой репликации (`test_replication.py`):
Скрипт проверяет симметричную репликацию: вставляет данные через разные узлы кольца и проверяет консистентность чтения и аналитики со всех пиров:
```bash
python3 scripts/test_replication.py
```
Вывод:
```
=================================================================
      GDB Leaderless Ring Replication Verification Suite     
=================================================================
[*] Probing leaderless ring peers and topology...
  [✓] Peer 1 at http://127.0.0.1:8847 is UP | Role: Peer | RF=3 | Mode=SYNC
  [✓] Peer 2 at http://127.0.0.1:8846 is UP | Role: Peer | RF=3 | Mode=SYNC
  [✓] Peer 3 at http://127.0.0.1:8845 is UP | Role: Peer | RF=3 | Mode=SYNC

[1/4] Creating Schema via Peer 1 (Broadcast DDL)...
  -> DDL Result: ok | ok

[2/4] Performing Symmetric Ingestion Across Different Peers...
  -> Inserting vertices 101..104 through Peer 1...
  -> Inserting vertices 105..108 through Peer 2...
  -> Inserting vertices 109..112 and connecting edges through Peer 3...
  -> Triggering compaction via Peer 2...

[3/4] Validating Consistency Across All Ring Peers...
  Peer 1 (:8847): 12 edges found
  Peer 2 (:8846): 12 edges found
  Peer 3 (:8845): 12 edges found

[4/4] Verifying Parallel PageRank Analytics across All Peers...
  PageRank Result Rows: Peer 1 = 14 | Peer 2 = 14 | Peer 3 = 14

=================================================================
[PASS] Leaderless Ring Replication is FULLY OPERATIONAL!
Symmetric writes from all peers processed and replicated across the hash ring.
=================================================================
```

### 4. Многопоточный стресс-тест с проверкой Prometheus (`stress_test.py`):
Генерирует высокую конкурентную нагрузку, замеряет QPS и задержки, а затем проверяет дельту метрик в Prometheus:
```bash
python3 scripts/stress_test.py --concurrency 8 --queries 2000
```
Фактические показатели на Apple M5:
```
Throughput:  7,455.6 QPS
Latency p50: 1.01 ms
Latency p95: 1.62 ms
Latency p99: 2.08 ms
Prometheus Delta: +2,000 queries registered on /metrics
```

### 5. Математическая валидация 12 алгоритмов графовой аналитики (`graph_analytics_validation.py`):
Сверяет вывод алгоритмов GDB с эталонным математическим расчетом на детерминированном тестовом графе (PageRank, Louvain, WCC, Triangle Count, SSSP, Jaccard Similarity и др.):
```bash
python3 scripts/graph_analytics_validation.py
```
Вывод:
```
[✓] TEST 1: PageRank convergence passed.
[✓] TEST 2: PageRank relative rank passed (Influencer > Hub > Periphery).
[✓] TEST 3: Louvain community count passed (2 distinct communities).
[✓] TEST 4: WCC component count passed (2 isolated components).
[✓] TEST 5: Triangle count ground-truth passed (Triangle=1, Periphery=0).
[✓] TEST 6: SSSP shortest paths ground-truth passed.
[✓] TEST 7: SSSP unreachable node distance passed (-1.0).
[✓] TEST 8: Jaccard similarity ground-truth passed (0.5000).
[✓] TEST 9: Cosine similarity ground-truth passed (0.7071).
[✓] TEST 10: Common neighbors count passed.
[✓] ALL 10 GRAPH ANALYTICS GROUND-TRUTH TESTS PASSED!
### 6. Высокоскоростной загрузчик на Python и Polars (`gdb-py-client`):
Для параллельной сверхбыстрой загрузки миллионов вершин и ребер разработан отдельный клиент [**gdb-py-client**](https://github.com/1orgar/gdb-py-client):
- **Стек:** **Polars** + **PyArrow Flight**.
- **Scatter-Ingest по токенам кольца:** клиент автоматически опрашивает топологию кольца через `/cluster`, разбивает Polars DataFrame по формуле `u % N` с помощью векторизованных выражений и стримит пачки RecordBatch параллельно через порт клиентского Arrow Flight (`:8860+`) прямо в целевые ноды.
- **Пример использования:**
```python
import polars as pl
from gdb_client import GdbClient

# Подключение к кластеру (автообнаружение кольца и портов)
client = GdbClient(seed_url="http://localhost:8847")

# Загрузка вершин через Polars Dataframe
df_v = pl.DataFrame({"id": [1, 2, 3], "name": ["Alice", "Bob", "Charlie"], "age": [30, 25, 35]})
client.scatter_ingest_vertices(df_v, tag="User")

# Загрузка связей параллельно на целевые ноды
df_e = pl.DataFrame({"src": [1, 2], "dst": [2, 3]})
client.scatter_ingest_edges(df_e, edge_type="FOLLOWS")

# Быстрый запрос через Flight do_get
batch = client.query_flight("MATCH (a:User)-[:FOLLOWS]->(b:User) RETURN a.name, b.name;")
print(batch.to_pandas())
```

---

## 🎮 Unity 3D WebGL Визуализатор Графа (`clients/unity-visualizer`)
Для исследовательской визуализации масштабных графов разработан интерактивный 3D-модуль на Unity WebGL:
- **Расположение:** [`clients/unity-visualizer`](clients/unity-visualizer).
- **Скрипт симуляции:** [`GraphVisualizer.cs`](clients/unity-visualizer/Assets/Scripts/GraphVisualizer.cs) с трехмерным силовым алгоритмом (Coulomb repulsion + Hooke spring attraction).
- **Двунаправленный мост (GdbBridge.jslib):**
  - Web UI -> Unity: `UpdateGraphData(nodesJson, edgesJson)` передает граф из ответа Cypher.
  - Unity -> Web UI: `OnNodeSelected(nodeId)` передает фокус на узел, открывая карточку в Inspector.

---

## 🏗 Структура проекта (Cargo Workspace)

```
gdb/
├── bin/                          # Релизные бинарники под ARM Mac (Apple Silicon)
│   ├── gdb-server                # Сервер ноды (Arrow Flight + HTTP + Multi-Raft + Metal GPU)
│   ├── gdb-cli                   # Интерактивная консоль с интроспекцией и health-check
│   └── gdb-studio                # Web UI & аналитический воркспейс
├── bin/amd64/                    # Релизные бинарники под AMD64 (x86_64)
│   ├── gdb-server                # Сервер ноды под x86_64
│   ├── gdb-cli                   # Интерактивная консоль под x86_64
│   └── gdb-studio                # Web UI & воркспейс под x86_64
├── docs/                         # Подробная документация
│   └── SERVER_AND_QUERY_GUIDE.md # Архитектура, конфигурация сервера и справочник запросов
├── scripts/                      # Скрипты управления кластером и тестирования (Pure Python 3)
│   ├── start_cluster.sh          # Запуск 3-узлового кластера (ARM Mac)
│   ├── stop_cluster.sh           # Остановка ARM Mac кластера
│   ├── start_cluster_amd64.sh    # Запуск 3-узлового кластера (AMD64)
│   ├── stop_cluster_amd64.sh     # Остановка AMD64 кластера
│   ├── start_studio.sh           # Запуск GDB Studio Web UI (:3000)
│   ├── stop_studio.sh            # Остановка GDB Studio
│   ├── data_loader.py            # Генерация и заливка масштабных графов
│   ├── benchmark_suite.py        # Замер пропускной способности и задержек
│   ├── test_replication.py       # Сквозной тест репликации между нодами
│   ├── stress_test.py            # Высоконагруженный стресс-тест с метриками Prometheus
│   └── graph_analytics_validation.py # Математическая верификация 12 алгоритмов
├── crates/
│   ├── gdb-core/                 # Базовые типы данных, ID, Arrow Schema Bridge
│   ├── gdb-storage/              # Dual-Store (Chunked-CSR + Delta MemTable + Arrow)
│   ├── gdb-wal/                  # Локальный Raft WAL с контрольными суммами CRC32
│   ├── gdb-raft/                 # Multi-Raft и Meta-Raft консенсус
│   ├── gdb-parser/               # openCypher / GQL / CALL-парсер
│   ├── gdb-planner/              # Векторизованный физический исполнитель
│   ├── gdb-flight/               # Apache Arrow Flight gRPC & MPP Shuffle Exchange
│   ├── gdb-s3/                   # S3 / MinIO Tiered Storage & Parquet сериализация
│   ├── gdb-gpu/                  # Metal Compute Shaders (Apple Silicon) & CUDA диспетчер
│   ├── gdb-analytics/            # Пакет Nebula Enterprise Analytics (12 алгоритмов)
│   ├── gdb-server/               # Демон ноды СУБД (HTTP REST + Flight + Prometheus)
│   ├── gdb-cli/                  # Консольный REPL с интроспекцией
│   └── gdb-studio/               # Автономный Web UI сервер
```

---

## Лицензия
Apache-2.0
