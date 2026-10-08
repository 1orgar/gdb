[English](OPERATIONS_GUIDE.md) | [Русский](OPERATIONS_GUIDE_RU.md)

# GDB Operations Guide: Установка кластера, загрузка больших данных и графовые расчеты

Полное руководство по эксплуатации распределенной in-memory графовой СУБД **GDB** (высокопроизводительный аналог Nebula Graph / Nebula Enterprise) версии **v0.4.0**.

---

## 1. Установка и запуск кластера

### 1.1. Системные требования и тюнинг ОС
Для достижения экстремальной производительности (десятки миллионов операций в секунду):
```bash
# 1. Увеличить лимиты на файловые дескрипторы
ulimit -n 65535

# 2. Убедиться, что Rust toolchain настроен
source "$HOME/.cargo/env"
cargo --version
```

### 1.2. Быстрый запуск 3-узлового кластера скриптами

В репозитории предусмотрены автоматические скрипты с поддержкой динамического поиска бинарников (в `bin/` или `target/release/`) и опциями GPU:

```bash
# ARM Mac (Apple Silicon):
./scripts/start_cluster.sh --rf 3 --sync

# Включение аппаратного ускорения Apple Metal:
./scripts/start_cluster.sh --rf 3 --sync --enable-gpu true --gpu-offload-threshold 10000

# Linux AMD64 (x86_64) / NVIDIA CUDA:
./scripts/start_cluster_amd64.sh --rf 3 --sync

# Остановка кластера:
./scripts/stop_cluster.sh
```

### 1.3. Локальный запуск кластера из 3 узлов вручную
Каждый узел СУБД является симметричным: принимает клиентские запросы, управляет своими Multi-Raft партициями и предоставляет Arrow Flight сервис.

```bash
# Нода 1 (Координатор + Шарды 0..7)
cargo run --release --bin gdb-server -- \
  --node-id 1 \
  --partitions 8 \
  --port 8848 \
  --http-port 8847 \
  --wal-dir ./data/node1/wal \
  --peers http://127.0.0.1:8846,http://127.0.0.1:8845 \
  --replication-factor 3 \
  --replication-mode sync \
  --enable-gpu false

# Нода 2
cargo run --release --bin gdb-server -- \
  --node-id 2 \
  --partitions 8 \
  --port 8849 \
  --http-port 8846 \
  --wal-dir ./data/node2/wal \
  --peers http://127.0.0.1:8847,http://127.0.0.1:8845 \
  --replication-factor 3 \
  --replication-mode sync \
  --enable-gpu false

# Нода 3
cargo run --release --bin gdb-server -- \
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

### 1.4. Развертывание в Docker Compose с S3 (MinIO)
В корень репозитория включен `docker-compose.yml`, развертывающий 3 ноды GDB и S3-хранилище MinIO:

```bash
# Запуск кластера и S3 в фоне
docker compose up -d

# Проверка статуса сервисов
docker compose ps

# Web-консоль MinIO S3: http://localhost:9001 (логин: minioadmin, пароль: minioadminpassword)
# Arrow Flight порт GDB: localhost:8848
```

---

## 2. Загрузка больших данных (High-Throughput Bulk Ingestion)

В GDB реализована двухуровневая архитектура **Dual-Store**:
1. Мутации пишутся в память в lock-free буфер **Delta MemTable** и локальный WAL со скоростью более **30 миллионов ребер в секунду**.
2. По завершении пачки вызывается **Compaction**, упаковывающий ребра в непрерывный кэш-выровненный **Chunked-CSR**, готовый для миллисекундных обходов и GPU-расчетов.

### 2.1. Сквозной скрипт массовой загрузки и расчетов
Для тестирования загрузки 1,000,000 ребер и 50,000 вершин запустите встроенный пайплайн:

```bash
cargo run -p gdb-server --example benchmark_and_bulk_load --release
```

**Фактическая скорость на Apple M5:**
- Загрузка 50,000 вершин со свойствами в Arrow: **16.3 мс** (`3,065,588 вершин/сек`).
- Загрузка 1,000,000 ребер в Delta MemTable: **31.5 мс** (`31.73 МИЛЛИОНА ребер/сек`).
- Компактизация 1,000,000 связей в плотный CSR: **92.2 мс**.

### 2.2. Загрузка данных через интерактивную консоль (`gdb-cli`)
Запустите CLI:
```bash
cargo run --release --bin gdb-cli
```

Создайте схему, вторичные индексы и добавьте данные:
```sql
-- 1. Создание схемы вершин и связей
CREATE VERTEX User (name STRING, age INT64);
CREATE EDGE FOLLOWS ();

-- 2. Создание вторичного индекса для O(1) поиска
CREATE INDEX ON :User(name);

-- 3. Вставка вершин
INSERT VERTEX User (id, name, age) VALUES (1, 'Alice', 30);
INSERT VERTEX User (id, name, age) VALUES (2, 'Bob', 25);
INSERT VERTEX User (id, name, age) VALUES (3, 'Charlie', 35);
INSERT VERTEX User (id, name, age) VALUES (4, 'Dave', 22);

-- 4. Вставка связей
INSERT EDGE FOLLOWS FROM 1 TO 2;
INSERT EDGE FOLLOWS FROM 2 TO 3;
INSERT EDGE FOLLOWS FROM 3 TO 1;
INSERT EDGE FOLLOWS FROM 3 TO 4;

-- 5. Атомарное обновление и слияние (DML)
MATCH (u:User {name: 'Alice'}) SET u.age = 31;
MERGE (u:User {name: 'Eve', age: 28});

-- 6. Вызов компактизации в CSR (для максимизации скорости обхода)
compact;
```

---

## 3. Произведение расчетов и Графовая Аналитика

GDB поддерживает три уровня аналитических вычислений:
1. **Шаблонные Cypher-запросы (MATCH, Filter, Expand, DML, Indexes, Explain)** для точечной и k-hop аналитики.
2. **Агрегации и пагинация** (`COUNT`, `SUM`, `AVG`, `MIN`, `MAX`, `DISTINCT`, `ORDER BY`, `SKIP`, `LIMIT`).
3. **Nebula Enterprise Analytics Suite (`CALL algo.<name>`)** для глобальных алгоритмов над всем графом с GPU-ускорением.

### 3.1. Выполнение запросов Cypher (openCypher / GQL)
```sql
-- k-hop поиск связей с фильтрацией по свойствам
MATCH (a:User)-[:FOLLOWS]->(b:User)-[:FOLLOWS]->(c:User)
WHERE a.age >= 25
RETURN a.name, b.name, c.name
ORDER BY a.name ASC
LIMIT 10;

-- Агрегация данных
MATCH (a:User)-[:FOLLOWS]->(b:User)
RETURN a.name, COUNT(b) AS followers
ORDER BY followers DESC;

-- Инспекция физического плана исполнения (EXPLAIN)
EXPLAIN MATCH (a:User)-[:FOLLOWS]->(b:User)
WHERE a.name = 'Alice'
RETURN b.name;
```

---

### 3.2. Запуск графовых алгоритмов корпоративного пакета (Enterprise Suite)

Все алгоритмы возвращают данные в векторном формате **Apache Arrow** (`RecordBatch`), что гарантирует нулевой оверхед на сериализацию:

#### А. Кластеризация и Сообщества (Community Detection)
```sql
-- 1. Louvain Community Detection (максимизация модулярности, GPU/SIMD)
CALL algo.louvain({max_iter: 10})
YIELD vertex_id, community_id;

-- 2. WCC (Weakly Connected Components - поиск изолированных подграфов, GPU/SIMD)
CALL algo.wcc()
YIELD vertex_id, component_id;

-- 3. SCC (Strongly Connected Components - ориентированная связность)
CALL algo.scc()
YIELD vertex_id, component_id;

-- 4. Triangle Count & Local Clustering Coefficient (LCC, GPU/SIMD)
CALL algo.triangleCount()
YIELD vertex_id, triangles;

-- 5. K-Core декомпозиция (поиск плотных ядер графа)
CALL algo.kCore()
YIELD vertex_id, coreness;
```

#### Б. Центральность и Ранжирование (Centrality & Ranking)
```sql
-- 6. PageRank (демпфирование 0.85, 20 итераций, проверка сходимости, GPU SpMV)
-- На 1 млн ребер отрабатывает за 6 миллисекунд!
CALL algo.pageRank({damping: 0.85, max_iter: 20, tolerance: 0.0001})
YIELD vertex_id, score;

-- 7. Betweenness Centrality (алгоритм Брандеса - поиск ключевых мостов)
CALL algo.betweenness({normalized: true})
YIELD vertex_id, betweenness;

-- 8. Closeness Centrality (гармоническая близость)
CALL algo.closeness()
YIELD vertex_id, closeness;

-- 9. Degree Centrality (In/Out/Total степени)
CALL algo.degree()
YIELD vertex_id, in_degree, out_degree, total_degree;
```

#### В. Кратчайшие пути и Сходство вершин (Pathfinding & Similarity)
```sql
-- 10. SSSP (Кратчайшие расстояния от заданной вершины ко всем остальным)
CALL algo.sssp({source: 1})
YIELD vertex_id, distance;

-- 11. Метрики сходства (Жаккар, косинусное сходство и общие соседи)
CALL algo.similarity({node1: 1, node2: 2})
YIELD jaccard, common_neighbors;
```

---

## 4. Аппаратное ускорение на GPU (Metal на Mac / CUDA на Linux)

GDB поддерживает высокопроизводительный GPU-пайплайн:
- **Настройка флагов запуска:**
  - `--enable-gpu true` — активирует GPU диспетчер (по умолчанию `false`).
  - `--gpu-device <ID>` — выбор конкретного графического ускорителя при наличии нескольких GPU (например, `--gpu-device 0`).
  - `--gpu-offload-threshold <N>` — порог переключения на GPU (по умолчанию 10,000 ребер).
- **На Apple Silicon (M-серия):** Архитектура **Unified Memory (UMA)** объединяет RAM процессора и видеокарты. GPU выполняет параллельный BFS, PageRank, WCC, Louvain и Triangle Counting **без необходимости копировать память по шине PCIe (Zero-Copy)**.
- **На Linux / NVIDIA:** Нативный CUDA бэкенд с поддержкой Tesla V100, A100, H100 и RTX GPU.
- **Адаптивный диспетчер (Cost-Based Dispatcher):**
  - Запросы размером $< 10,000$ ребер исполняются на CPU (минимизация задержки диспетчеризации).
  - Массовые обходы и аналитика ($> 10,000$ элементов) автоматически переключаются на ядра Metal/CUDA.

---

## 5. Персистентность и создание снапшотов в S3

Для долговременного хранения графа и быстрого Disaster Recovery:
1. Нода периодически сбрасывает компактный CSR в **Apache Parquet** в S3:
   `partitions/p{id}/snapshot_v{version}.parquet`.
2. При перезапуске нода скачивает Parquet-снапшот из S3 и накатывает последние записи из локального Raft WAL (`gdb-wal`).
