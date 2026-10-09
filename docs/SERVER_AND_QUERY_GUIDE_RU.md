[English](SERVER_AND_QUERY_GUIDE.md) | [Русский](SERVER_AND_QUERY_GUIDE_RU.md)

# GDB: Руководство по запуску, конфигурированию сервера и справочник запросов

Полная техническая документация по эксплуатации распределенной in-memory графовой СУБД **GDB** (высокопроизводительный аналог Nebula Graph / Nebula Enterprise) версии **v0.4.1**.

---

## Часть 1. Запуск и конфигурирование сервера

Серверный узел GDB (`gdb-server`) объединяет:
- Двухуровневое in-memory хранилище (Chunked-CSR топология + Delta MemTable с MVCC).
- Вторичные индексы свойств вершин (`DashMap`) с автоподдержанием при мутациях.
- Движок распределенного консенсуса Multi-Raft с локальным append-only WAL журналом.
- Аппаратный ускоритель вычислений (Apple Metal Compute UMA Zero-Copy / NVIDIA CUDA / CPU SIMD).
- Транспортный уровень: Apache Arrow Flight (gRPC) + HTTP REST API (:8847).
- Сетевую репликацию мутаций на ведомые узлы кластера (`/raft/replicate`).
- Эндпоинт интроспекции схемы каталога (`/schema`).
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
| `--http-port` | — | — | `u16` | `8847` | Сетевой порт **HTTP REST API**. Принимает запросы пользователей (`POST /query`), инспекцию схемы (`GET /schema`), межрепликационные вызовы (`POST /replicate`), отдает метрики Prometheus (`GET /metrics`), статус кластера (`GET /cluster`), статус ресурсов (`GET /resources`) и health-check (`GET /health`). |
| `--wal-dir` | — | — | `path` | `./data/wal` | Каталог на диске для журнала упреждающей записи **Write-Ahead Log (WAL)** с верификацией контрольных сумм CRC32. |
| `--peers` | — | — | `string` | *(пусто)* | Список HTTP REST адресов других участников кольца через запятую (например: `"http://127.0.0.1:8846,http://127.0.0.1:8845"`). На основе этого списка нода динамически строит топологию кольца. |
| `--replication-factor` | `-r` | `GDB_REPLICATION_FACTOR` | `u32` | `3` | **Фактор репликации кольца (RF):**<br>• `1` — чистое шардирование без дублирования (режим MPP, $\sum\text{RAM}$).<br>• `k` — частичная репликация на $k$ последовательных узлов кольца.<br>• `N` — полное зеркалирование (100% данных на всех узлах). Автоматически ограничивается количеством активных нод. |
| `--replication-mode` | — | `GDB_REPLICATION_MODE` | `string` | `sync` | **Режим репликации:**<br>• `sync` (или `synchronous`) — синхронный: координатор ожидает параллельного подтверждения записи от всех целевых реплик перед ответом клиенту.<br>• `async` (или `asynchronous`) — асинхронный: координатор моментально отвечает клиенту, передавая мутацию репликам в фоновых задачах `tokio::spawn`. |
| `--enable-gpu` | — | `GDB_ENABLE_GPU` | `bool` | `false` | **Флаг включения GPU:** По умолчанию выключено (`false`) в пользу векторизованного CPU SIMD. |
| `--gpu-device` | — | `GDB_GPU_DEVICE` | `usize` | `0` | **Выбор конкретного GPU:** Индекс графического процессора в мульти-GPU системах (Tesla V100, A100, H100, RTX). |
| `--gpu-offload-threshold` | — | `GDB_GPU_THRESHOLD` | `usize` | `10000` | **Порог передачи на GPU:** Минимальное количество ребер графа для переключения вычислений на GPU. |
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
| `--nodes <N>` | `-n <N>` | `./scripts/start_cluster.sh --nodes 5` | Количество запускаемых узлов кластера (по умолчанию 3). Поддерживается любое число $\ge 1$. |
| `--rf <N>` | `-r <N>`, `--replication-factor <N>` | `./scripts/start_cluster.sh --rf 1` | Устанавливает фактор репликации для всех 3 узлов (допустимы значения `1`, `2`, `3`). |
| `--sync` | `sync`, `--replication-mode sync` | `./scripts/start_cluster.sh --sync` | Включает строгую синхронную репликацию мутаций с ожиданием кворума. |
| `--async` | `async`, `--replication-mode async` | `./scripts/start_cluster.sh --async` | Включает фоновую асинхронную репликацию для максимального TPS. |
| `--sharding` | `--sharded`, `sharding` | `./scripts/start_cluster.sh --sharding` | Экспресс-алиас для `--rf 1 --sync` (чистый распределенный MPP кластер). |
| `--replication` | `--replicated`, `replication` | `./scripts/start_cluster.sh --replication` | Экспресс-алиас для `--rf 3 --sync` (полная синхронная репликация). |
| `--enable-gpu <bool>` | `--gpu <bool>` | `./scripts/start_cluster.sh --enable-gpu true` | Включает или выключает GPU ускорение на узлах кластера. |
| `--gpu-device <ID>` | `--device <ID>` | `./scripts/start_cluster.sh --gpu-device 0` | Выбирает целевой графический процессор. |
| `--gpu-offload-threshold <N>` | `--threshold <N>` | `./scripts/start_cluster.sh --gpu-offload-threshold 5000` | Устанавливает порог передачи k-hop обходов и аналитики на GPU. |

#### Сводная таблица портов стандартного 3-узлового кластера:

| Узел кластера | Роль в кольце | HTTP REST API | Internal Flight | Client Flight Port | Локальный WAL каталог | Файл логов |
| :--- | :--- | :---: | :---: | :---: | :--- | :--- |
| **Peer 1** | Peer / Coordinator | `http://127.0.0.1:8847` | `:8848` | `:8860` | `./data/node1/wal` | `./logs/node1.log` |
| **Peer 2** | Peer / Storage | `http://127.0.0.1:8846` | `:8849` | `:8861` | `./data/node2/wal` | `./logs/node2.log` |
| **Peer 3** | Peer / Storage | `http://127.0.0.1:8845` | `:8850` | `:8862` | `./data/node3/wal` | `./logs/node3.log` |
| **Web Studio UI**| Графический интерфейс | `http://localhost:3000`| — | — | — | `./logs/studio.log` |

---

### 1.3. Архитектура: Беслидерная кольцевая репликация (Leaderless Hash Ring)

В GDB все узлы кластера являются **равноправными пирами (Symmetric Peers)**, организованными в логическое кольцо хэширования (в стиле Amazon Dynamo / Apache Cassandra):
- **Отсутствие единой точки отказа (SPOF):** Нет фиксированных ролей Leader / Follower. Любой узел кластера может принимать клиентские запросы на чтение и запись, выступая в роли **Координатора (Coordinator)**.
- **Определение реплика-сета для ключа:** Для вершины $u$ или ребра $u \to v$ первичный узел определяется по формуле:
  $$\text{Primary Node Index} = u \pmod N$$
  Данные реплицируются на первичный узел и последующие $\text{RF} - 1$ узлов по ходу кольца:
  $$\text{Replicas}(u) = \{ (\text{Primary Node Index} + i) \pmod N \mid i \in [0, \text{RF} - 1] \}$$

#### Проверка состояния кольца через CLI
```sql
SHOW CLUSTER;
SHOW RESOURCES;
```

---

### 1.4. Аппаратное GPU-ускорение вычислений (Metal & CUDA)

Сервер автоматически определяет аппаратную платформу при запуске:
- **Apple Silicon (M1/M2/M3/M4/M5 на macOS):** Активируется **Apple Metal Compute Backend** с архитектурой **Unified Memory (UMA Zero-Copy)**.
- **Linux x86_64 / amd64 c NVIDIA GPU:** Активируется **NVIDIA CUDA Backend** с поддержкой Tesla V100, A100, H100, RTX.
- **CPU SIMD Fallback:** При `--enable-gpu false` или отсутствии GPU используется параллельный векторизованный бэкенд на Rayon.

#### Поддерживаемые GPU-ядра и алгоритмы:
- `parallel_bfs_step` / `cuda_bfs_frontier_kernel` — параллельное расширение фронтира волны BFS.
- `parallel_pagerank_step` / `cuda_pagerank_spmv_kernel` — векторное распределение массы PageRank по ребрам (SpMV).
- `louvain_step` / `cuda_louvain_kernel` — оптимизация модулярности сообществ.
- `wcc_step` / `cuda_wcc_kernel` — распространение идентификаторов компонент связности.
- `triangle_count_step` / `cuda_triangle_count_kernel` — пересечение списков смежности соседей.

---

## Часть 2. Справочник и примеры запросов

Подключение к кластеру осуществляется через:
- Консоль CLI: `./bin/gdb-cli`
- Web UI Workspace: `./scripts/start_studio.sh` (http://localhost:3000)
- HTTP REST API: `POST http://localhost:8847/query` с телом `{"query": "..."}`
- Arrow Flight External Port: `:8860` через `gdb-py-client`

---

### 2.1. DDL: Определение схемы и Вторичные Индексы

```sql
-- 1. Создание вершинных тегов (Vertex Tags)
CREATE VERTEX User (name STRING, age INT64);
CREATE VERTEX Device (model STRING, ram_gb INT64);
CREATE VERTEX Server (ip STRING, cores INT64);

-- 2. Создание типов ребер (Edge Types)
CREATE EDGE FOLLOWS ();
CREATE EDGE KNOWS ();
CREATE EDGE CONNECTS ();

-- 3. Создание вторичных индексов по свойствам (Secondary Property Indexes)
-- Обеспечивают O(1) поиск в физическом операторе IndexScan
CREATE INDEX ON :User(name);
CREATE INDEX ON :User(age);

-- 4. Изменение тегов вершин и типов ребер (ALTER)
ALTER VERTEX User ADD (email STRING, country STRING);
ALTER VERTEX User DROP (country);
ALTER EDGE FOLLOWS ADD (since INT64);
ALTER EDGE FOLLOWS DROP (since);

-- 5. Удаление элементов схемы
DROP INDEX ON :User(age);
DROP VERTEX Device;
DROP EDGE CONNECTS;
```

---

### 2.2. DML: Вставка, Мутации и Слияние (Cypher DML)

```sql
-- 1. Вставка вершин (одиночная и пакетная Bulk Insert)
INSERT VERTEX User (id, name, age) VALUES (1, 'Alice', 30);

INSERT VERTEX User (id, name, age) VALUES 
  (2, 'Bob', 25), 
  (3, 'Charlie', 35), 
  (4, 'Dave', 28);

-- 2. Вставка ребер
INSERT EDGE FOLLOWS FROM 1 TO 2;

INSERT EDGE FOLLOWS VALUES 
  (2, 3), 
  (3, 1), 
  (3, 4);

-- 3. Мутация свойств (openCypher SET)
MATCH (u:User {name: 'Alice'}) SET u.age = 31, u.status = 'active';

-- 4. Удаление вершин и связанных связей (openCypher DELETE / DETACH DELETE)
MATCH (u:User {name: 'Dave'}) DELETE u;
MATCH (u:User {name: 'Dave'}) DETACH DELETE u;

-- 5. Идемпотентная вставка или слияние (openCypher MERGE)
MERGE (u:User {name: 'Eve', age: 26});

-- 6. Принудительная компактизация мутационного буфера в Chunked-CSR
compact;
```

---

### 2.3. openCypher / GQL: Шаблоны, Multi-Hop, Агрегации и Пагинация

```sql
-- 1-Hop обход с проекцией свойств:
MATCH (a:User)-[:FOLLOWS]->(b:User)
RETURN a.name, b.name;

-- Multi-Hop: Переменная глубина обхода с диапазоном (Star Cast *min..max):
MATCH (a:User)-[:FOLLOWS*1..3]->(b:User)
WHERE a.id = 1
RETURN a.name, b.name;

-- Конвейеризация промежуточных вычислений через оператор WITH:
MATCH (u:User)
WITH u.department AS dept, count(u) AS team_size, avg(u.salary) AS avg_sal
WHERE team_size >= 2
RETURN dept, team_size, avg_sal
ORDER BY team_size DESC;

-- Передача переменных между шаблонами через WITH:
MATCH (a:User)-[:FOLLOWS]->(b:User)
WITH b.name AS followee, b.age AS followee_age
WHERE followee_age > 20
RETURN followee, followee_age;

-- Агрегации и Group By:
MATCH (a:User)-[:FOLLOWS]->(b:User)
RETURN a.name, COUNT(b) AS followers, AVG(b.age) AS avg_age
ORDER BY followers DESC;

-- Устранение дубликатов (DISTINCT), пагинация (SKIP, LIMIT):
MATCH (a:User)-[:FOLLOWS]->(b:User)
RETURN DISTINCT b.name
ORDER BY b.name ASC
SKIP 10
LIMIT 20;

-- Инспекция физического плана запроса (EXPLAIN):
EXPLAIN MATCH (a:User)-[:FOLLOWS]->(b:User)
WHERE a.name = 'Alice'
RETURN b.name;
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

В консоли `./bin/gdb-cli` доступны интерактивные команды:

```text
SHOW CLUSTER              - Вывод топологии нод кластера, ролей, портов и задержек.
SHOW RESOURCES            - Вывод расхода памяти, количества вершин и ребер (CSR vs MemTable).
SHOW GPU                  - Статус аппаратного ускорения Metal/CUDA, память UMA, пороги диспетчеризации.
:connect <http://url>     - Переключение целевого адреса кластера на лету.
compact                   - Ручной вызов компактизации CSR на целевом сервере.
exit / quit               - Выход из консоли.
```

---

## Часть 3. Внешний Arrow Flight сервис и клиент Polars (`gdb-py-client`)

Для высокоскоростной параллельной загрузки данных с аналитических воркстейшенов в GDB выделен отдельный порт Flight (`--client-flight-port`, по умолчанию `:8860`).

### Пример работы с Python клиентом:
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

---

## Часть 4. Тестирование, верификация и покрытие кода

В GDB встроен непрерывный конвейер тестирования и измерения покрытия:

```bash
# 1. Запуск всех тестов рабочего пространства (юнит-, интеграционные и E2E)
cargo test --workspace

# 2. Автоматизированный запуск пайплайна покрытия (cargo-llvm-cov)
./scripts/coverage.sh

# 3. Просмотр интерактивного HTML-отчета
cargo llvm-cov --workspace --html --open
```

Подробные метрики покрытия по всем подсистемам и тестам приведены в документе **[COVERAGE.md](COVERAGE.md)**.
