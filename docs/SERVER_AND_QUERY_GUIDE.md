# GDB: Руководство по запуску, конфигурированию сервера и справочник запросов

Полная техническая документация по эксплуатации распределенной in-memory графовой СУБД **GDB** (аналог Nebula Graph / Nebula Enterprise).

---

## Часть 1. Запуск и конфигурирование сервера

Серверный узел GDB (`gdb-server`) объединяет:
- Двухуровневое in-memory хранилище (Chunked-CSR топология + Delta MemTable с MVCC).
- Движок распределенного консенсуса Multi-Raft с локальным append-only WAL журналом.
- Аппаратный ускоритель вычислений (Apple Metal Compute UMA Zero-Copy / CPU SIMD).
- Транспортный уровень: Apache Arrow Flight (gRPC) + HTTP REST API (:8847).
- Сетевую репликацию мутаций на ведомые узлы кластера (`/raft/replicate`).
- Экспортер метрик Prometheus (`/metrics`).

### 1.1. Полный справочник параметров запуска `gdb-server`

```bash
gdb-server [OPTIONS]
```

#### Параметры командной строки (CLI Flags) & Переменные окружения (Environment Variables)

| Флаг CLI | Короткий | Переменная окружения | Тип | По умолчанию | Описание и назначение |
| :--- | :---: | :--- | :---: | :---: | :--- |
| `--node-id` | `-n` | — | `u64` | `1` | Уникальный числовой идентификатор узла в кольце кластера. Используется при расчете принадлежности токенов: $\text{Token} = (u \pmod N) + 1$. |
| `--partitions` | — | — | `u32` | `4` | Количество независимых Multi-Raft групп и локальных партиций графового хранилища на ноде. |
| `--port` | `-p` | — | `u16` | `8848` | Сетевой порт сервиса **Internal Apache Arrow Flight gRPC**. Обеспечивает векторный MPP обмен RecordBatch и межсетевой шаффл при распределенных запросах между узлами кластера. |
| `--client-flight-port` | — | `GDB_CLIENT_FLIGHT_PORT` | `u16` | `8860` | Сетевой порт **External Client Flight gRPC**. Обеспечивает высокоскоростную потоковую параллельную загрузку (`do_put`) и прямое исполнение Cypher запросов (`do_get`) для внешних клиентов (`gdb-py-client`). |
| `--http-port` | — | — | `u16` | `8847` | Сетевой порт **HTTP REST API**. Принимает запросы пользователей (`POST /query`), межрепликационные вызовы (`POST /replicate`), отдает метрики Prometheus (`GET /metrics`), статус кластера (`GET /cluster`), статус ресурсов (`GET /resources`) и health-check (`GET /health`). |
| `--wal-dir` | — | — | `path` | `./data/wal` | Каталог на диске для журнала упреждающей записи **Write-Ahead Log (WAL)** с верификацией контрольных сумм CRC32. |
| `--peers` | — | — | `string` | *(пусто)* | Список HTTP REST адресов других участников кольца через запятую (например: `"http://127.0.0.1:8846,http://127.0.0.1:8845"`). На основе этого списка нода динамически строит топологию кольца. |
| `--replication-factor` | `-r` | `GDB_REPLICATION_FACTOR` | `u32` | `3` | **Фактор репликации кольца (RF):**<br>• `1` — чистое шардирование без дублирования (режим MPP, $\sum\text{RAM}$).<br>• `k` — частичная репликация на $k$ последовательных узлов кольца.<br>• `N` — полное зеркалирование (100% данных на всех узлах). Автоматически ограничивается количеством активных нод. |
| `--replication-mode` | — | `GDB_REPLICATION_MODE` | `string` | `sync` | **Режим репликации:**<br>• `sync` (или `synchronous`) — синхронный: координатор ожидает параллельного подтверждения записи от всех целевых реплик перед ответом клиенту.<br>• `async` (или `asynchronous`) — асинхронный: координатор моментально отвечает клиенту, передавая мутацию репликам в фоновых задачах `tokio::spawn`. |
| `--s3-bucket` | — | `AWS_BUCKET` | `string` | *(пусто)* | Имя бакета AWS S3 или MinIO. При указании активирует модуль многоуровневого хранения (Tiered Storage) и команду `snapshot;`. |
| `--s3-endpoint` | — | `AWS_ENDPOINT` | `string` | *(пусто)* | Пользовательский URL S3-совместимого сервиса (например, `http://localhost:9000` для локального MinIO или Ceph). |
| `--s3-region` | — | `AWS_REGION` | `string` | `us-east-1` | Регион AWS S3 (например, `eu-central-1`, `us-east-1`). |
| *(credential)* | — | `AWS_ACCESS_KEY_ID` | `string` | *(пусто)* | Ключ доступа для аутентификации в S3/MinIO. |
| *(credential)* | — | `AWS_SECRET_ACCESS_KEY`| `string` | *(пусто)* | Секретный ключ для аутентификации в S3/MinIO. |
| *(logging)* | — | `RUST_LOG` | `string` | `info` | Уровень детализации логирования tracing (`error`, `warn`, `info`, `debug`, `trace`). |
| `--cluster-mode` | — | — | `string` | `ring` | Алиас совместимости (`ring`, `replication`, `sharding`). |

---

### 1.2. Параметры автоматических скриптов запуска (`start_cluster.sh` и `start_cluster_amd64.sh`)

Скрипты автоматизируют запуск 3-узлового локального кластера и поддерживают следующие аргументы командной строки:

| Флаг скрипта | Алиасы | Пример использования | Описание |
| :--- | :--- | :--- | :--- |
| `--rf <N>` | `-r <N>`, `--replication-factor <N>` | `./scripts/start_cluster.sh --rf 1` | Устанавливает фактор репликации для всех 3 узлов (допустимы значения `1`, `2`, `3`). |
| `--sync` | `sync`, `--replication-mode sync` | `./scripts/start_cluster.sh --sync` | Включает строгую синхронную репликацию мутаций с ожиданием кворума. |
| `--async` | `async`, `--replication-mode async` | `./scripts/start_cluster.sh --async` | Включает фоновую асинхронную репликацию для максимального TPS. |
| `--sharding` | `--sharded`, `sharding` | `./scripts/start_cluster.sh --sharding` | Экспресс-алиас для `--rf 1 --sync` (чистый распределенный MPP кластер). |
| `--replication` | `--replicated`, `replication` | `./scripts/start_cluster.sh --replication` | Экспресс-алиас для `--rf 3 --sync` (полная синхронная репликация). |

#### Сводная таблица портов стандартного 3-узлового кластера:

| Узел кластера | Роль в кольце | HTTP REST API | Internal Flight | Client Flight Port | Локальный WAL каталог | Файл логов |
| :--- | :--- | :---: | :---: | :---: | :--- | :--- |
| **Peer 1** | Peer / Coordinator | `http://127.0.0.1:8847` | `:8848` | `:8860` | `./data/node1/wal` | `./logs/node1.log` |
| **Peer 2** | Peer / Storage | `http://127.0.0.1:8846` | `:8849` | `:8861` | `./data/node2/wal` | `./logs/node2.log` |
| **Peer 3** | Peer / Storage | `http://127.0.0.1:8845` | `:8850` | `:8862` | `./data/node3/wal` | `./logs/node3.log` |
| **Web Studio UI**| Графический интерфейс | `http://localhost:3000`| — | — | — | `./logs/studio.log` |

---

### 1.2. Архитектура: Беслидерная кольцевая репликация (Leaderless Hash Ring)

В GDB все узлы кластера являются **равноправными пирами (Symmetric Peers)**, организованными в логическое кольцо хэширования (в стиле Amazon Dynamo / Apache Cassandra):
- **Отсутствие единой точки отказа (SPOF):** Нет фиксированных ролей Leader / Follower. Любой узел кластера может принимать клиентские запросы на чтение и запись, выступая в роли **Координатора (Coordinator)**.
- **Определение реплика-сета для ключа:** Для вершины $u$ или ребра $u \to v$ первичный узел определяется по формуле:
  $$\text{Primary Node Index} = u \pmod N$$
  Данные реплицируются на первичный узел и последующие $\text{RF} - 1$ узлов по ходу кольца:
  $$\text{Replicas}(u) = \{ (\text{Primary Node Index} + i) \pmod N \mid i \in [0, \text{RF} - 1] \}$$

#### Фактор репликации (Replication Factor, RF)
1. **$\text{RF} = 1$ (Чистое шардирование без репликации):**
   - Данные сохраняются исключительно на первичном узле $(u \pmod N)$.
   - Каждая нода хранит свой уникальный срез графа. Общий объем графа равен суммарной оперативной памяти всех машин.
2. **$1 < \text{RF} < N$ (Частичная кольцевая репликация):**
   - Данные дублируются на $K$ последовательных узлах кольца.
3. **$\text{RF} = N$ (Полная репликация / High Availability):**
   - Все узлы содержат 100% топологии и атрибутов графа. Все графовые алгоритмы (PageRank, BFS, WCC) выполняются с 0 сетевых задержек локально на каждом узле.

#### Режимы синхронизации (Sync vs Async)
- **Синхронный режим (`--replication-mode sync` — по умолчанию):**
  Координатор параллельно рассылает мутации целевым репликам и ожидает подтверждения от всех узлов реплика-сета перед ответом клиенту. Гарантирует немедленную согласованность (Strong Consistency / Read-Your-Writes).
- **Асинхронный режим (`--replication-mode async`):**
  Координатор применяет запись локально и рассылает репликационные пакеты в фоновых задачах (`tokio::spawn`), моментально возвращая ответ клиенту. Обеспечивает максимальную пропускную способность (Ultra-High Throughput).

#### Примеры запуска кластера
```bash
# 1. Полная синхронная репликация (RF=3, SYNC):
./scripts/start_cluster.sh --rf 3 --sync

# 2. Асинхронная репликация с высокой пропускной способностью (RF=3, ASYNC):
./scripts/start_cluster.sh --rf 3 --async

# 3. Чистое распределенное шардирование без репликации (RF=1, SYNC):
./scripts/start_cluster.sh --rf 1 --sync
```

#### Проверка состояния кольца через CLI
```sql
SHOW CLUSTER;
SHOW RESOURCES;
```
Пример вывода `SHOW CLUSTER;`:
```text
=== GDB Leaderless Ring Cluster Topology ===
  Topology: Leaderless Hash Ring | Replication Factor: RF=3 | Mode: SYNC

+-----------------------+------+------------------+-----------------------+-------------+--------+---------+
| Node                  | Role | Ring Token Range | HTTP Endpoint         | Flight Port | Status | Latency |
+=======================++======++==================++=======================++=============++========++=========++
| Peer Node #1 (active) | Peer | u % 3 == 0       | http://127.0.0.1:8847 | 8848        | UP     | 0.10 ms |
| Peer Node #2          | Peer | u % 3 == 1       | http://127.0.0.1:8846 | 8849        | UP     | 0.23 ms |
| Peer Node #3          | Peer | u % 3 == 2       | http://127.0.0.1:8845 | 8850        | UP     | 0.17 ms |
+-----------------------+------+------------------+-----------------------+-------------+--------+---------+
```

---

### 1.3. Аппаратное GPU-ускорение вычислений

Сервер автоматически определяет аппаратную платформу при запуске:
- **Apple Silicon (M1/M2/M3/M4/M5 на macOS):** Активируется **Apple Metal Compute Backend**.
- **Linux x86_64 / amd64 c NVIDIA GPU:** Активируется **NVIDIA CUDA Backend** (`crates/gdb-gpu/src/cuda.rs`). Автоматически детектирует устройство через `/dev/nvidia0` или переменную `CUDA_PATH`.
- **CPU Fallback:** Если GPU не обнаружен, используется параллельный векторизованный **CPU SIMD Fallback** (Rayon + AVX2/AVX-512).

#### Особенности Unified Memory Architecture (UMA Zero-Copy):
На чипах Apple Silicon оперативная память CPU и графические ядра GPU физически объединены. Буферы топологии `ChunkedCsr` (`offsets`, `targets`) мапятся в память графического конвейера **напрямую без накладных расходов на копирование через PCIe шину**.

#### Адаптивный диспетчер:
- **Порог диспетчеризации (`threshold_edges`):** По умолчанию **10 000 ребер**.
- Небольшие OLTP-выборки ($< 10\,000$ ребер) выполняются на CPU без накладных расходов на инициализацию шейдеров Metal/CUDA.
- Массовые аналитические расчеты ($> 10\,000$ ребер) автоматически перенаправляются на вычислительные ядра GPU.

#### Поддерживаемые GPU-ядра (Metal Shading Language & CUDA Kernels):
- `parallel_bfs_step` / `cuda_bfs_frontier_kernel` — параллельное расширение фронтира волны BFS.
- `parallel_pagerank_step` / `cuda_pagerank_spmv_kernel` — векторное распределение массы PageRank по ребрам (SpMV).
- `cosine_jaccard_kernel` — параллельный расчет пересечения множеств соседей.

---

### 1.4. Tiered Storage: Персистентность в S3 / MinIO

GDB обеспечивает долговечность данных за счет многоуровневого хранения:
1. **Горячий слой (Hot In-Memory):** Оперативная память (Chunked-CSR + Delta MemTable).
2. **Теплый слой (Warm Local NVMe):** Журнал упреждающей записи WAL с проверкой целостности CRC32.
3. **Холодный слой (Cold Object Storage):** Хранилище S3 или MinIO в сжатом формате **Apache Parquet (ZSTD)**.

#### Конфигурация через переменные окружения:
```bash
export AWS_ACCESS_KEY_ID="minioadmin"
export AWS_SECRET_ACCESS_KEY="minioadminpassword"
export AWS_ENDPOINT="http://localhost:9000"
export AWS_BUCKET="gdb-snapshots"
export AWS_REGION="us-east-1"
```

#### Восстановление после сбоя:
1. Узел скачивает базовый снапшот `partition_{id}.parquet` из S3.
2. Десериализует столбчатые массивы Arrow в `ChunkedCsr`.
3. Доигрывает (Replay) последние незафиксированные в снапшоте записи из локального WAL журнала.

---

### 1.5. Мониторинг через Prometheus (`/metrics`)

Каждый серверный узел предоставляет метрики в формате OpenMetrics по адресу:
`http://<host>:<http_port>/metrics`

#### Основные метрики:
| Метрика | Тип | Описание |
| :--- | :--- | :--- |
| `gdb_uptime_seconds` | Gauge | Время непрерывной работы процесса (в секундах). |
| `gdb_queries_total{status="ok\|error"}` | Counter | Общее число выполненных запросов по статусам. |
| `gdb_query_duration_seconds` | Summary | Квантили задержки запросов (p50, p90, p99). |
| `gdb_vertices_total` | Gauge | Общее число вершин в хранилище. |
| `gdb_edges_total` | Gauge | Общее число активных ребер (CSR + Delta). |
| `gdb_memtable_edges_count` | Gauge | Некомпактизованные ребра в Delta MemTable. |
| `gdb_csr_edges_count` | Gauge | Ребра, сжатые в Chunked-CSR. |
| `gdb_compactions_total` | Counter | Количество выполненных компактизаций. |
| `gdb_raft_term` | Gauge | Текущий терм консенсуса Raft. |
| `gdb_raft_is_leader` | Gauge | Флаг лидера (1 — Leader, 0 — Follower). |
| `gdb_raft_replications_total` | Counter | Успешно реплицированные мутации на пиры. |
| `gdb_gpu_active` | Gauge | Статус активности аппаратного GPU (1 / 0). |
| `gdb_memory_allocated_bytes` | Gauge | Оценка выделенной памяти процесса. |

---

## Часть 2. Справочник и примеры запросов

Подключение к кластеру осуществляется через:
- Консоль CLI: `./bin/gdb-cli`
- Web UI Workspace: `./scripts/start_studio.sh` (http://localhost:3000)
- HTTP REST API: `POST http://localhost:8847/query` с телом `{"query": "..."}`

---

### 2.1. DDL: Определение схемы графа

```sql
-- 1. Создание вершинных тегов (Vertex Tags)
CREATE VERTEX User (name STRING, age INT64);
CREATE VERTEX Device (model STRING, ram_gb INT64);
CREATE VERTEX Server (ip STRING, cores INT64);

-- 2. Создание типов ребер (Edge Types)
CREATE EDGE FOLLOWS ();
CREATE EDGE KNOWS ();
CREATE EDGE CONNECTS ();
```

---

### 2.2. DML: Вставка и управление данными

```sql
-- 1. Вставка вершин (одиночная и пакетная Cypher Bulk Insert)
INSERT VERTEX User (id, name, age) VALUES (1, 'Alice', 30);

-- Пакетная вставка нескольких вершин в одном операторе (Bulk Insert):
INSERT VERTEX User (id, name, age) VALUES 
  (2, 'Bob', 25), 
  (3, 'Charlie', 35), 
  (4, 'Dave', 28);

-- 2. Вставка ребер (одиночная и пакетная Cypher Bulk Insert)
INSERT EDGE FOLLOWS FROM 1 TO 2;

-- Пакетная вставка нескольких ребер в одном операторе (Bulk Insert):
INSERT EDGE FOLLOWS VALUES 
  (2, 3), 
  (3, 1), 
  (3, 4);

-- 3. Принудительная компактизация мутационного буфера в Chunked-CSR
-- Рекомендуется вызывать после завершения пакетной загрузки данных:
compact;
```

---

### 2.3. openCypher / GQL: Шаблоны, k-hop и Multi-Hop обходы

```sql
-- 1-Hop обход с проекцией свойств:
MATCH (a:User)-[:FOLLOWS]->(b:User)
RETURN a.name, b.name;

-- 2-Hop обход (друзья друзей):
MATCH (a:User)-[:FOLLOWS]->(b:User)-[:FOLLOWS]->(c:User)
RETURN a.name, b.name, c.name;

-- Multi-Hop: Переменная глубина обхода с диапазоном (Star Cast *min..max):
MATCH (a:User)-[:FOLLOWS*1..3]->(b:User)
WHERE a.id = 1
RETURN a.name, b.name;

-- Multi-Hop: Любой тип связи с переменной глубиной:
MATCH (a:User)-[*1..2]->(b:User)
RETURN a.id, b.id;

-- Обход с фильтрацией (WHERE) и ограничением объема (LIMIT):
MATCH (a:User)-[:FOLLOWS]->(b:User)
WHERE a.age > 25
RETURN a.name, b.name
LIMIT 50;
```

---

### 2.4. Пакет графовой аналитики (Nebula Enterprise Analytics Suite)

Все алгоритмы выполняются параллельно на CPU SIMD или GPU:

#### 1. PageRank (Важность вершин):
```sql
CALL algo.pageRank({damping: 0.85, max_iter: 20}) YIELD vertex_id, score;
```

#### 2. Louvain (Кластеризация сообществ по модулярности):
```sql
CALL algo.louvain({max_iter: 10}) YIELD vertex_id, community_id;
```

#### 3. WCC (Слабосвязные компоненты):
```sql
CALL algo.wcc() YIELD vertex_id, component_id;
```

#### 4. SCC (Сильносвязные компоненты Tarjan):
```sql
CALL algo.scc() YIELD vertex_id, component_id;
```

#### 5. Triangle Count & Local Clustering Coefficient (LCC):
```sql
CALL algo.triangleCount() YIELD vertex_id, triangles;
```

#### 6. K-Core Decomposition (Выделение плотных ядер графа):
```sql
CALL algo.kCore() YIELD vertex_id, coreness;
```

#### 7. Betweenness Centrality (Центральность по посредничеству Brandes):
```sql
CALL algo.betweenness() YIELD vertex_id, betweenness;
```

#### 8. Closeness Centrality (Центральность по близости):
```sql
CALL algo.closeness() YIELD vertex_id, closeness;
```

#### 9. Degree Centrality (Степени вершин):
```sql
CALL algo.degree() YIELD vertex_id, in_degree, out_degree;
```

#### 10. SSSP (Поиск кратчайшего пути от заданного источника):
```sql
CALL algo.sssp(1) YIELD vertex_id, distance;
```

#### 11. Similarity (Коэффициенты Жаккара и косинусного сходства соседей):
```sql
CALL algo.similarity({node1: 1, node2: 2}) YIELD jaccard, cosine;
```

---

### 2.5. Системные команды CLI

В консоли `./bin/gdb-cli` доступны интерактивные команды интроспекции:

```text
SHOW CLUSTER              - Вывод топологии нод кластера, ролей (Leader/Follower), портов и задержек.
SHOW RESOURCES            - Вывод расхода памяти, количества вершин и ребер (CSR vs MemTable).
SHOW GPU                  - Статус аппаратного ускорения Metal/CUDA, память UMA, пороги диспетчеризации.
:connect <http://url>     - Переключение целевого адреса кластера на лету.
compact                   - Ручной вызов компактизации CSR на целевом сервере.
exit / quit               - Выход из консоли.
```

---

## Часть 3. Набор тестовых скриптов (Python Stdlib)

В каталоге `scripts/` содержатся инструменты для тестирования и эксплуатации без внешних библиотек:

1. **[`scripts/data_loader.py`](file:///Users/kirill/Documents/projects/gdb/scripts/data_loader.py):**
   Массовая генерация графа по закону масштабирования Power-Law и параллельная заливка в базу.
   ```bash
   python3 scripts/data_loader.py --vertices 10000 --edges 100000
   ```

2. **[`scripts/benchmark_suite.py`](file:///Users/kirill/Documents/projects/gdb/scripts/benchmark_suite.py):**
   Комплексный бенчмарк задержек k-hop обходов и скорости алгоритмов аналитики.
   ```bash
   python3 scripts/benchmark_suite.py --samples 500 --concurrency 4
   ```

3. **[`scripts/test_replication.py`](file:///Users/kirill/Documents/projects/gdb/scripts/test_replication.py):**
   Верификация сквозной синхронизации данных между лидером (Нода 1) и фолловерами (Ноды 2 и 3).
   ```bash
   python3 scripts/test_replication.py
   ```

4. **[`scripts/stress_test.py`](file:///Users/kirill/Documents/projects/gdb/scripts/stress_test.py):**
   Многопоточный стресс-тест с отслеживанием приращения Prometheus-метрик.
   ```bash
   python3 scripts/stress_test.py --concurrency 8 --duration 5 --write-ratio 0.3
   ```

---

## Часть 4. Внешний Arrow Flight сервис и клиент Polars ([gdb-py-client](https://github.com/1orgar/gdb-py-client))

Для высокоскоростной параллельной загрузки данных с аналитических воркстейшенов в GDB выделен отдельный порт Flight (`--client-flight-port`, по умолчанию `:8860`).

### 4.1. Архитектура взаимодействия:
1. **Разделение трафика:**
   - Порт `--port 8848+`: только внутренний MPP shuffle exchange между узлами кластера.
   - Порт `--client-flight-port 8860+`: внешний клиентский шлюз (в перспективе поддерживает авторизацию и токены).
2. **Streaming do_put Ingestion:**
   - Клиент передает дескриптор `GdbFlightDescriptor` (JSON: `{"type": "vertex", "tag": "User"}` или `{"type": "edge", "edge_type": "FOLLOWS"}`).
   - Данные стримятся в формате Arrow `RecordBatch` и напрямую трансформируются в `DeltaMemTable` без сериализации в текст Cypher.
3. **Scatter-Ingest на клиенте (`gdb-py-client`):**
   - Библиотека на Python разбивает Polars DataFrame по формуле `u % N` (Primary token в хеш-кольце).
   - Векторизованные чанки отправляются параллельно в `client-flight-port` конкретных целевых узлов, минимизируя сетевые пересылки между нодами.

### 4.2. Пример работы с Python клиентом:
```python
import polars as pl
from gdb_client import GdbClient

# Подключение к семени кластера
client = GdbClient(seed_url="http://localhost:8847")

# Массовая вставка 1,000,000 вершин напрямую через Arrow Flight do_put:
df_vertices = pl.DataFrame({
    "id": range(1, 1_000_001),
    "name": [f"User_{i}" for i in range(1, 1_000_001)],
    "age": [20 + (i % 50) for i in range(1, 1_000_001)],
})
client.scatter_ingest_vertices(df_vertices, tag="User")

# Массовая вставка ребер:
df_edges = pl.DataFrame({
    "src": range(1, 1_000_000),
    "dst": range(2, 1_000_001),
})
client.scatter_ingest_edges(df_edges, edge_type="FOLLOWS")
```

5. **[`scripts/graph_analytics_validation.py`](file:///Users/kirill/Documents/projects/gdb/scripts/graph_analytics_validation.py):**
   Математическая проверка точности всех 12 аналитических алгоритмов на эталонных топологиях.
   ```bash
   python3 scripts/graph_analytics_validation.py
   ```
