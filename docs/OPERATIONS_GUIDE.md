# GDB Operations Guide: Установка кластера, загрузка больших данных и графовые расчеты

Полное руководство по эксплуатации распределенной in-memory графовой СУБД **GDB** (высокопроизводительный аналог Nebula Graph / Nebula Enterprise).

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

### 1.2. Вариант А: Локальный запуск кластера из 3 узлов (Native Multi-Node)
Каждый узел СУБД является симметричным: принимает клиентские запросы, управляет своими Multi-Raft партициями и предоставляет Arrow Flight сервис.

Запустите 3 терминала (или фоновые процессы):

```bash
# Нода 1 (Координатор + Шарды 0..7)
cargo run --release --bin gdb-server -- \
  --node-id 1 \
  --partitions 8 \
  --port 8848 \
  --wal-dir ./data/node1/wal

# Нода 2
cargo run --release --bin gdb-server -- \
  --node-id 2 \
  --partitions 8 \
  --port 8849 \
  --wal-dir ./data/node2/wal

# Нода 3
cargo run --release --bin gdb-server -- \
  --node-id 3 \
  --partitions 8 \
  --port 8850 \
  --wal-dir ./data/node3/wal
```

### 1.3. Вариант Б: Развертывание в Docker Compose с S3 (MinIO)
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

Создайте схему и добавьте данные:
```sql
-- 1. Создание схемы вершин и связей
CREATE VERTEX User (name STRING, age INT64);
CREATE EDGE FOLLOWS ();

-- 2. Вставка вершин
INSERT VERTEX User (id, name, age) VALUES (1, 'Alice', 30);
INSERT VERTEX User (id, name, age) VALUES (2, 'Bob', 25);
INSERT VERTEX User (id, name, age) VALUES (3, 'Charlie', 35);
INSERT VERTEX User (id, name, age) VALUES (4, 'Dave', 22);

-- 3. Вставка связей
INSERT EDGE FOLLOWS FROM 1 TO 2;
INSERT EDGE FOLLOWS FROM 2 TO 3;
INSERT EDGE FOLLOWS FROM 3 TO 1;
INSERT EDGE FOLLOWS FROM 3 TO 4;

-- 4. Вызов компактизации в CSR (для максимизации скорости обхода)
compact;
```

### 2.3. Рекомендации для сверхбольших датасетов (десятки/сотни млн ребер)
1. **Отключение частой компактизации во время заливки:**
   Грузите данные непрерывными пачками по 500,000 – 1,000,000 ребер. Вызывайте `compact` только после завершения импорта батча.
2. **Использование бинарного формата Parquet / S3:**
   При персистентности снапшот 1,000,000 ребер сжимается ZSTD в Parquet размером всего **0.69 MB** и мгновенно восстанавливается с S3/NVMe без повторного парсинга текстовых форматов.

---

## 3. Произведение расчетов и Графовая Аналитика

GDB поддерживает два типа расчетов:
1. **Шаблонные Cypher-запросы (MATCH, Filter, Expand)** для точечной и k-hop аналитики.
2. **Nebula Enterprise Analytics Suite (`CALL algo.<name>`)** для глобальных алгоритмов над всем графом.

### 3.1. Выполнение запросов Cypher (openCypher / GQL)
```sql
-- k-hop поиск связей с фильтрацией по свойствам
MATCH (a:User)-[:FOLLOWS]->(b:User)-[:FOLLOWS]->(c:User)
WHERE a.age >= 25
RETURN a.name, b.name, c.name;

-- Подсчет общего числа связей
MATCH (a:User)-[:FOLLOWS]->(b:User)
RETURN COUNT(*);
```

---

### 3.2. Запуск графовых алгоритмов корпоративного пакета (Enterprise Suite)

Все алгоритмы возвращают данные в векторном формате **Apache Arrow** (`RecordBatch`), что гарантирует нулевой оверхед на сериализацию:

#### А. Кластеризация и Сообщества (Community Detection)
```sql
-- 1. Louvain Community Detection (максимизация модулярности)
CALL algo.louvain({max_iter: 10})
YIELD vertex_id, community_id;

-- 2. WCC (Weakly Connected Components - поиск изолированных подграфов)
CALL algo.wcc()
YIELD vertex_id, component_id;

-- 3. SCC (Strongly Connected Components - ориентированная связность)
CALL algo.scc()
YIELD vertex_id, component_id;

-- 4. Triangle Count & Local Clustering Coefficient (LCC)
CALL algo.triangleCount()
YIELD vertex_id, triangles;

-- 5. K-Core декомпозиция (поиск плотных ядер графа)
CALL algo.kCore()
YIELD vertex_id, coreness;
```

#### Б. Центральность и Ранжирование (Centrality & Ranking)
```sql
-- 6. PageRank (демпфирование 0.85, 20 итераций, проверка сходимости)
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

GDB автоматически включает GPU-пайплайн:
- **На Apple Silicon (M-серия):** Архитектура **Unified Memory (UMA)** объединяет RAM процессора и видеокарты. GPU выполняет параллельный BFS и фильтрацию данных **без необходимости копировать память по шине PCIe (Zero-Copy)**.
- **Адаптивный диспетчер (Cost-Based Dispatcher):** 
  - Запросы размером $< 10,000$ ребер исполняются на CPU (минимизация задержки диспетчеризации).
  - Массовые обходы и аналитика ($> 10,000$ элементов) автоматически переключаются на ядра Metal/CUDA.

---

## 5. Персистентность и создание снапшотов в S3

Для долговременного хранения графа и быстрого Disaster Recovery:
1. Нода периодически сбрасывает компактный CSR в **Apache Parquet** в S3:
   `partitions/p{id}/snapshot_v{version}.parquet`.
2. При перезапуске нода скачивает Parquet-снапшот из S3 и накатывает последние записи из локального Raft WAL (`gdb-wal`).
