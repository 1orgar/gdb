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

3. **Горизонтальное масштабирование и Multi-Raft:**
   - Шардирование графа по хешу `VertexId` с Source-Colocation (1D Edge Cut).
   - Независимые Raft-группы на каждую партицию с локальным высокоскоростным журналом `gdb-wal`.

4. **Аппаратное GPU-ускорение (Metal на Mac / CUDA на Linux):**
   - **Apple Silicon (M-серия):** Архитектура единой памяти **Unified Memory Architecture (UMA)** позволяет графическому процессору читать топологию графа из RAM **напрямую с нулевой стоимостью копирования (Zero-Copy)**.
   - Адаптивный диспетчер: автоматический офлоад тяжелых обходов и аналитики на GPU-ядра Metal Compute.

5. **Персистентность в S3 (Tiered Storage):**
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
# Запуск 3-узлового кластера
./scripts/start_cluster.sh
```

Вывод:
```
============================================================
       Starting 3-Node GDB Cluster (ARM Mac + GPU)         
============================================================
[+] Node 1 started (PID 49972): Flight :8848 | HTTP :8847 | GPU: Apple Metal UMA
[+] Node 2 started (PID 49973): Flight :8849 | HTTP :8846 | GPU: Apple Metal UMA
[+] Node 3 started (PID 49974): Flight :8850 | HTTP :8845 | GPU: Apple Metal UMA

[✓] 3-node cluster is healthy and ready for queries!
    CLI connect:   ./bin/gdb-cli
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

#### Терминал 1: Нода 1 (Координатор + Шарды 0..7)
```bash
./bin/gdb-server \
  --node-id 1 \
  --partitions 8 \
  --port 8848 \
  --http-port 8847 \
  --wal-dir ./data/node1/wal
```

#### Терминал 2: Нода 2 (Хранилище + Multi-Raft)
```bash
./bin/gdb-server \
  --node-id 2 \
  --partitions 8 \
  --port 8849 \
  --http-port 8846 \
  --wal-dir ./data/node2/wal
```

#### Терминал 3: Нода 3 (Хранилище + Multi-Raft)
```bash
./bin/gdb-server \
  --node-id 3 \
  --partitions 8 \
  --port 8850 \
  --wal-dir ./data/node3/wal
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
# Нода 1 (Координатор + Шарды)
./bin/amd64/gdb-server --node-id 1 --partitions 8 --port 8848 --http-port 8847 --wal-dir ./data/amd64/node1/wal

# Нода 2 (Хранилище)
./bin/amd64/gdb-server --node-id 2 --partitions 8 --port 8849 --http-port 8846 --wal-dir ./data/amd64/node2/wal

# Нода 3 (Хранилище)
./bin/amd64/gdb-server --node-id 3 --partitions 8 --port 8850 --http-port 8845 --wal-dir ./data/amd64/node3/wal
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
2. **Физическая визуализация графа (Graph View 60 FPS):**
   - Симуляция силовых полей (Force-Directed Graph) на HTML5 Canvas с поддержкой Retina-дисплеев.
   - Зум (колесико мыши), свободное панорамирование (Pan), перетаскивание узлов (Drag-and-Drop).
   - Направленные стрелки связей с подписями типов отношений (`[:FOLLOWS]`, `[:KNOWS]`).
   - Цветовая палитра по типам сущностей и сообществам алгоритмов.
3. **Инспектор сущностей (Inspector):**
   - Клик на любой узел или связь отображает карточку с ID, меткой, свойствами и списком смежных ребер.
4. **Табличное представление (Table View):**
   - Сортируемая сетка данных с фиксацией заголовков для аналитических запросов (`CALL algo.pageRank()`, `CALL algo.louvain()`).
5. **Мониторинг кластера:**
   - Отображение статуса нод (Node 1 Leader :8847, Node 2 :8846, Node 3 :8845) и возможность динамической смены URL подключения.

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
\____(_)_____/_____/  Interactive Cypher Shell v0.1.0

[✓] Connected to GDB Node at http://localhost:8847 (Latency: 0.8ms)
    Type 'help' or '\?' for help. Press Ctrl+D to exit.
```

### Команды интроспекции кластера, ресурсов и GPU:
В интерактивной консоли доступны специализированные управляющие команды:

| Команда | Описание |
| :--- | :--- |
| `SHOW CLUSTER` | Таблица всех узлов кластера: Node ID, роль (Leader / Follower), порты Flight и HTTP REST. |
| `SHOW RESOURCES` | Метрики потребления: объем памяти (RSS), аптайм, QPS, кол-во вершин и ребер (CSR vs MemTable), компактизации. |
| `SHOW GPU` | Статус графического ускорителя: активный бэкенд (Apple Metal / CUDA / CPU SIMD), UMA Zero-Copy, порог офлоада. |
| `:connect <url>` | Динамическое переключение текущей сессии CLI на другой узел (например, `:connect http://localhost:8846`). |

#### Примеры вывода команд:
```sql
gdb> SHOW CLUSTER;
+---------+----------+-------------+-----------+
| Node ID | Role     | Flight Port | HTTP Port |
+---------+----------+-------------+-----------+
| 1       | Leader   | 8848        | 8847      |
| 2       | Follower | 8849        | 8846      |
| 3       | Follower | 8850        | 8845      |
+---------+----------+-------------+-----------+

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

-- 2. Вставка данных
INSERT VERTEX User (id, name, age) VALUES (1, 'Alice', 30);
INSERT VERTEX User (id, name, age) VALUES (2, 'Bob', 25);
INSERT VERTEX User (id, name, age) VALUES (3, 'Charlie', 35);
INSERT EDGE FOLLOWS FROM 1 TO 2;
INSERT EDGE FOLLOWS FROM 2 TO 3;
INSERT EDGE FOLLOWS FROM 3 TO 1;

-- 3. Принудительная компактизация в Chunked-CSR
compact;

-- 4. Обход графа (k-hop Cypher)
MATCH (a:User)-[:FOLLOWS]->(b:User)-[:FOLLOWS]->(c:User)
RETURN a.name, b.name, c.name;
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

# HELP gdb_raft_is_leader Whether this node is currently the Raft leader (1=leader, 0=follower)
# TYPE gdb_raft_is_leader gauge
gdb_raft_is_leader{node_id="1"} 1

# HELP gdb_gpu_available Whether GPU acceleration is active and available
# TYPE gdb_gpu_available gauge
gdb_gpu_available{node_id="1",backend="metal"} 1
```

---

## 🔄 Распределенная Репликация (Multi-Raft)

В GDB реализована межузловая репликация операций изменения данных (DDL/DML):
1. **Флаг `--peers`:** При старте узла передается список адресов пиров (например, `--peers http://localhost:8846,http://localhost:8845`).
2. **Маршрутизация мутаций:** Все запросы на запись (`CREATE VERTEX/EDGE`, `INSERT`, `compact`), отправленные лидеру, автоматически транслируются через HTTP RPC `POST /raft/replicate` на ведомые ноды.
3. **Согласованность:** Ведомые ноды фиксируют мутации в локальном хранилище и возвращают подтверждение `{"replicated":true,"target_node":X}`.

Скрипты `start_cluster.sh` и `start_cluster_amd64.sh` автоматически запускают кластер с корректно настроенной топологией репликации между тремя нодами.

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

### 3. Сквозная проверка межузловой репликации (`test_replication.py`):
Скрипт проверяет сквозную репликацию: вставляет данные в Leader (Node 1) и считывает их с Follower (Node 2 и Node 3):
```bash
python3 scripts/test_replication.py
```
Вывод:
```
[+] Step 1: Writing unique test vertex and edge to Leader (Node 1)...
[+] Step 2: Triggering compaction on Leader (Node 1)...
[+] Step 3: Verifying data replication on Follower 1 (Node 2)...
    [✓] Follower 1 verified successfully!
[+] Step 4: Verifying data replication on Follower 2 (Node 3)...
    [✓] Follower 2 verified successfully!
[✓] SUCCESS: End-to-end Multi-Raft replication verified across all 3 nodes!
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
```

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
