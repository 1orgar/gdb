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

### 1.1. Параметры командной строки `gdb-server`

```bash
gdb-server [OPTIONS]
```

| Флаг | Тип | По умолчанию | Описание |
| :--- | :--- | :--- | :--- |
| `-n, --node-id` | `u64` | `1` | Уникальный числовой идентификатор узла в кластере. |
| `--partitions` | `u32` | `4` | Общее количество партиций шардирования графа в кластере. |
| `-p, --port` | `u16` | `8848` | Порт Arrow Flight gRPC сервиса (векторный обмен MPP). |
| `--http-port` | `u16` | `8847` | Порт HTTP REST API (запросы, метрики, репликация). |
| `--wal-dir` | `path` | `./data/wal` | Каталог для локального Write-Ahead Log (WAL) с CRC32. |
| `--peers` | `string` | *(пусто)* | Список HTTP-адресов других нод кластера через запятую. |
| `--cluster-mode` | `string` | `replication` | Режим работы кластера: `replication` (HA / зеркалирование) или `sharding` (распределенный 1D Edge Cut). |
| `--s3-bucket` | `string` | *(пусто)* | Имя бакета AWS S3 / MinIO для создания снапшотов. |
| `--s3-endpoint` | `string` | *(пусто)* | Пользовательский S3 эндпоинт (например, `http://localhost:9000` для MinIO). |
| `--s3-region` | `string` | `us-east-1` | AWS S3 регион. |

---

### 1.2. Режимы работы кластера: Репликация vs Шардирование

GDB поддерживает два фундаментальных режима распределения данных:

#### 1. Режим полной репликации (`--cluster-mode replication` — по умолчанию)
- **Суть:** 100% топологии графа и атрибутов хранятся в памяти каждого узла.
- **Поведение:** Лидер (Node 1) принимает DDL и DML запросы и прозрачно транслирует их на ведомые узлы через эндпоинт `POST /raft/replicate`.
- **Преимущества:** Максимальная скорость графовой аналитики (PageRank, WCC, Shortest Path) — любые многошаговые графовые обходы выполняются с **0 сетевых задержек**, целиком в локальной оперативной памяти / GPU ноды. Отказоустойчивость: выход из строя нод не приводит к потере данных.
- **Запуск кластера:**
  ```bash
  ./scripts/start_cluster.sh                # или ./scripts/start_cluster.sh --replication
  ```

#### 2. Режим горизонтального шардирования (`--cluster-mode sharding`)
- **Суть:** Граф равномерно распределяется по $N$ узлам по схеме **1D Edge Cut (Source-Colocation)**:
  $$\text{Target Node} = (u \pmod N) + 1$$
- **Поведение:**
  - **DML (INSERT / DELETE):** При отправке вершины или исходящего ребра на любой узел кластера, нода вычисляет ответственный узел по `u % N`. Если запрос попал не на свой узел, он прозрачно пересылается HTTP-клиентом на владельца шарда.
  - **DDL (CREATE VERTEX LABEL, CREATE EDGE TYPE):** Автоматически бродкастится на все узлы кластера для поддержания идентичной схемы.
- **Преимущества:** Позволяет хранить графы колоссального объема, суммарный размер которых превышает объем оперативной памяти одного физического сервера.
- **Запуск кластера:**
  ```bash
  ./scripts/start_cluster.sh --sharding
  ```

#### Проверка текущего режима кластера через CLI
```bash
./bin/gdb-cli -e "SHOW CLUSTER;"
# или в интерактивном режиме:
SHOW CLUSTER;
SHOW RESOURCES;
```
В выводе отображается:
- `Architecture Mode: REPLICATION | Shards: All Partitions (Full Replication)` или
- `Architecture Mode: SHARDING | Shards: Partitions where u % 3 == 0`

---

### 1.3. Аппаратное GPU-ускорение вычислений

Сервер автоматически определяет аппаратную платформу при запуске:
- **Apple Silicon (M1/M2/M3/M4/M5 на macOS):** Активируется **Apple Metal Compute Backend**.
- **Linux x86_64 / amd64:** Активируется параллельный векторизованный **CPU SIMD Fallback** (Rayon + AVX2/AVX-512) или CUDA.

#### Особенности Unified Memory Architecture (UMA Zero-Copy):
На чипах Apple Silicon оперативная память CPU и графические ядра GPU физически объединены. Буферы топологии `ChunkedCsr` (`offsets`, `targets`) мапятся в память графического конвейера **напрямую без накладных расходов на копирование через PCIe шину**.

#### Адаптивный диспетчер:
- **Порог диспетчеризации (`threshold_edges`):** По умолчанию **10 000 ребер**.
- Небольшие OLTP-выборки ($< 10\,000$ ребер) выполняются на CPU без накладных расходов на инициализацию шейдеров Metal.
- Массовые аналитические расчеты ($> 10\,000$ ребер) автоматически перенаправляются на вычислительные ядра GPU.

#### Поддерживаемые GPU-ядра (Metal Shading Language):
- `parallel_bfs_step` — параллельное расширение фронтира волны BFS.
- `parallel_pagerank_step` — векторное распределение массы PageRank по ребрам.
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
-- 1. Вставка вершин
INSERT VERTEX User (id, name, age) VALUES (1, 'Alice', 30);
INSERT VERTEX User (id, name, age) VALUES (2, 'Bob', 25);
INSERT VERTEX User (id, name, age) VALUES (3, 'Charlie', 35);

-- 2. Вставка ребер
INSERT EDGE FOLLOWS FROM 1 TO 2;
INSERT EDGE FOLLOWS FROM 2 TO 3;
INSERT EDGE FOLLOWS FROM 3 TO 1;

-- 3. Принудительная компактизация мутационного буфера в Chunked-CSR
-- Рекомендуется вызывать после завершения пакетной загрузки данных:
compact;
```

---

### 2.3. openCypher / GQL: Шаблоны и k-hop обходы

```sql
-- 1-Hop обход с проекцией свойств:
MATCH (a:User)-[:FOLLOWS]->(b:User)
RETURN a.name, b.name;

-- 2-Hop обход (друзья друзей):
MATCH (a:User)-[:FOLLOWS]->(b:User)-[:FOLLOWS]->(c:User)
RETURN a.name, b.name, c.name;

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

5. **[`scripts/graph_analytics_validation.py`](file:///Users/kirill/Documents/projects/gdb/scripts/graph_analytics_validation.py):**
   Математическая проверка точности всех 12 аналитических алгоритмов на эталонных топологиях.
   ```bash
   python3 scripts/graph_analytics_validation.py
   ```
