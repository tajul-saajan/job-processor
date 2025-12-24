# Worker Architecture & Bounded Concurrency

*A deep dive into job acquisition, semaphore-based backpressure, and connection pool management*

---

This document provides a deep technical dive into the worker architecture and bounded concurrency implementation.

> **Prerequisites**: Read [README.md](./README.md) first for project context and high-level architecture.

**Key concepts covered:**
- Three-layer concurrency model (DB pool → Workers → Semaphore)
- PostgreSQL row-level locking (`FOR UPDATE SKIP LOCKED`)
- Semaphore-based bounded concurrency
- Connection pool sizing formula
- Backpressure mechanics
- Configuration for different workloads
- Monitoring and observability

---

## The Three-Layer Concurrency Model

Most job processing systems have an implicit concurrency model. This system makes it explicit through three coordinated constraints:

```
┌─────────────────────────────────────────────────────┐
│  Layer 1: DATABASE CONNECTION POOL                  │
│  └─ Hard resource limit (15 connections)            │
└─────────────────────────────────────────────────────┘
                     ↓
┌─────────────────────────────────────────────────────┐
│  Layer 2: WORKER LOOPS                              │
│  └─ Job acquisition throughput (3 workers)          │
└─────────────────────────────────────────────────────┘
                     ↓
┌─────────────────────────────────────────────────────┐
│  Layer 3: SEMAPHORE                                 │
│  └─ Bounded parallel execution (5 permits)          │
└─────────────────────────────────────────────────────┘
```

All three must be balanced. Get it wrong, and you'll see connection pool exhaustion (too few connections) or idle workers (too many workers).

---

## Layer 1: Database Connection Pool

### The Formula

```
MAX_DB_CONNECTIONS = NUM_WORKERS + MAX_CONCURRENT_JOBS + API_BUFFER
                   = 3           + 5                  + 7
                   = 15
```

### Why This Matters

In Node.js, I've debugged production incidents where:
- Connection pool too small → API timeouts under load
- Connection pool too large → Database connection limit hit

**This formula makes the tradeoff explicit.**

### Connection Usage Breakdown

```
┌──────────────────────────────────────────────────────┐
│         DATABASE CONNECTION POOL (15)                │
├──────────────────────────────────────────────────────┤
│                                                       │
│  Workers (3):                                         │
│  ┌─────┐ ┌─────┐ ┌─────┐              = 3 conns     │
│  │ W1  │ │ W2  │ │ W3  │                             │
│  └─────┘ └─────┘ └─────┘                             │
│                                                       │
│  Processing Tasks (5):                                │
│  ┌─────┐ ┌─────┐ ┌─────┐ ┌─────┐ ┌─────┐            │
│  │ T1  │ │ T2  │ │ T3  │ │ T4  │ │ T5  │  = 5 conns │
│  └─────┘ └─────┘ └─────┘ └─────┘ └─────┘            │
│                                                       │
│  API Handlers (variable):                             │
│  ┌─────┐ ┌─────┐ ┌─────┐                             │
│  │ API │ │ API │ │ API │  ...           = 0-7 conns  │
│  └─────┘ └─────┘ └─────┘                             │
│                                                       │
│  Total in use: 3 + 5 + (0-7) = 8-15 connections      │
└──────────────────────────────────────────────────────┘
```

> **Node.js equivalent**: See [README.md](./README.md#complete-nodejs-implementation) for Node.js implementation patterns.

---

## Layer 2: Worker Loops

### What Workers Do

Workers are **fast job acquisition loops**. Each worker:

1. Calls `acquire_next_job()` (10-50ms, database round-trip)
2. Gets a semaphore permit (may wait if all 5 in use)
3. Spawns a processing task
4. Immediately loops back to step 1

**Workers don't block on job processing.** They spawn tasks and keep acquiring.

### Why 3 Workers < 5 Semaphore Permits

This is counter-intuitive but optimal:

```
Time    Worker 1         Worker 2         Worker 3         Permits Used
────────────────────────────────────────────────────────────────────────
0.00s   Acquire Job 1    Acquire Job 2    Acquire Job 3    0/5
0.01s   Spawn Task 1 ✓   Spawn Task 2 ✓   Spawn Task 3 ✓   3/5
0.02s   Acquire Job 4    Acquire Job 5    Acquire Job 6    3/5
0.03s   Spawn Task 4 ✓   Spawn Task 5 ✓   WAIT (full!)     5/5 ← FULL
0.04s   Acquire Job 7    Acquire Job 8    [waiting...]     5/5
        WAIT (full!)     WAIT (full!)

1.00s   [Task 3 completes - releases permit]                4/5
1.01s   [waiting done]   [waiting done]   Spawn Task 6 ✓   5/5
```

**Key insight:** 3 workers can easily saturate 5 permits because job acquisition (~10ms) is **much faster** than job processing (1-5 seconds).

```
Acquisition rate:  ~100 jobs/second per worker
Processing rate:   ~0.2-1 job/second per permit

3 workers can acquire 300 jobs/second
5 permits can process ~1-5 jobs/second

Workers are never the bottleneck.
```

### Why Not More Workers?

**With 5 workers, 5 permits:**

```
Worker 1: Acquires Job 1 → Gets permit → Spawns → Acquires Job 6 → WAIT
Worker 2: Acquires Job 2 → Gets permit → Spawns → Acquires Job 7 → WAIT
Worker 3: Acquires Job 3 → Gets permit → Spawns → Acquires Job 8 → WAIT
Worker 4: Acquires Job 4 → Gets permit → Spawns → Acquires Job 9 → WAIT
Worker 5: Acquires Job 5 → Gets permit → Spawns → Acquires Job 10 → WAIT

All 5 workers blocked waiting for permits!
```

**Problems:**
- ❌ 5 database connections held by idle workers
- ❌ High contention on the `jobs` table (row locks)
- ❌ No benefit—still only 5 jobs processing
- ❌ Wasted resources

> **Node.js equivalent**: See [README.md](./README.md#complete-nodejs-implementation) for Node.js implementation patterns.

---

## Layer 3: Semaphore (The Critical Piece)

### What Is a Semaphore?

A semaphore is a **resource counter**. It has N permits. To process a job, you must acquire a permit. When done, you release it.

```rust
let semaphore = Arc::new(Semaphore::new(5)); // 5 permits

// Worker loop
loop {
    let job = acquire_next_job().await?;

    // This blocks if all 5 permits in use
    let permit = semaphore.acquire_owned().await?;

    tokio::spawn(async move {
        process_job(job).await;
        drop(permit); // Auto-released here
    });
}
```

### Why This Works

1. **Bounded concurrency**: Never more than 5 jobs processing
2. **Backpressure at the right place**: Workers wait for permits, not database connections
3. **Automatic cleanup**: Permits released via RAII, even on panic
4. **Non-bypassable**: Can't spawn a task without a permit

### What Happens Under Load

**Scenario: 100 jobs arrive instantly**

```
Time    Workers              Semaphore        Processing
─────────────────────────────────────────────────────────
0.0s    W1: Acquire Job 1    Permits: 5/5     0 jobs
0.1s    W2: Acquire Job 2    Permits: 4/5     1 job
0.2s    W3: Acquire Job 3    Permits: 3/5     2 jobs
0.3s    W1: Acquire Job 4    Permits: 2/5     3 jobs
0.4s    W2: Acquire Job 5    Permits: 1/5     4 jobs
0.5s    W3: Acquire Job 6    Permits: 0/5     5 jobs ← FULL
0.6s    W1: Acquire Job 7    Permits: 0/5     5 jobs
        [W1 BLOCKED waiting for permit]
0.7s    W2: Acquire Job 8    Permits: 0/5     5 jobs
        [W2 BLOCKED waiting for permit]
0.8s    W3: Acquire Job 9    Permits: 0/5     5 jobs
        [W3 BLOCKED waiting for permit]

1.0s    [Job 1 completes]    Permits: 1/5     4 jobs
1.1s    W1: Spawn Job 7      Permits: 0/5     5 jobs
        [W1 unblocked, immediately acquires Job 10]

... and so on
```

**Jobs 10-100 remain safely in the database.** No OOM. No unbounded spawning.

> **Node.js equivalent**: See [README.md](./README.md#complete-nodejs-implementation) for Node.js implementation patterns.

---

## The Job Acquisition Pattern

### PostgreSQL Row Locking

This is the secret sauce. Jobs are acquired atomically using:

```sql
SELECT id, name, status, created_at, updated_at
FROM jobs
WHERE status = 'new'
ORDER BY created_at ASC
LIMIT 1
FOR UPDATE SKIP LOCKED
```

**Key clauses:**

1. **`FOR UPDATE`**: Locks the row so no other worker can acquire it
2. **`SKIP LOCKED`**: If a row is already locked, skip it instead of waiting
3. **Transaction**: The SELECT and UPDATE happen atomically

### The Atomic State Machine

```
┌─────────────────────────────────────────────────────┐
│                 Job State Machine                    │
├─────────────────────────────────────────────────────┤
│                                                      │
│   ┌─────┐                                           │
│   │ new │ ←─ Job created via API                    │
│   └──┬──┘                                           │
│      │                                               │
│      │ Worker acquires (FOR UPDATE SKIP LOCKED)     │
│      ↓                                               │
│   ┌────────────┐                                    │
│   │ processing │ ←─ Owned by exactly 1 worker       │
│   └─────┬──────┘                                    │
│         │                                            │
│         │ Processing completes                      │
│         ↓                                            │
│    ┌─────────┐                                      │
│    │ success │  or  │ failed │                      │
│    └─────────┘      └────────┘                      │
│                                                      │
└─────────────────────────────────────────────────────┘
```

**Invariant:** A job can only be in one state at a time, and state transitions are atomic.

### Why This Matters

In Node.js with Redis-backed queues, I've debugged:
- Duplicate job processing (race conditions)
- Lost jobs (failed to ACK)
- Stale locks (worker died, lock never released)

**PostgreSQL row locks solve all three:**
- ✅ No duplicates (row locked)
- ✅ No lost jobs (persisted in DB)
- ✅ No stale locks (released on connection close)

> **Node.js equivalent**: See [README.md](./README.md#complete-nodejs-implementation) for Node.js implementation patterns.

---

## Backpressure in Action

### What Is Backpressure?

Backpressure is **pushback applied when a system is overloaded**.

**Without backpressure:**
```
Requests → Process everything immediately → OOM crash
```

**With backpressure:**
```
Requests → Queue safely → Process at sustainable rate
```

### Where Backpressure Happens

In this system, backpressure occurs at **two boundaries**:

#### 1. Semaphore Boundary (Primary)

```rust
// Worker tries to get permit
let permit = semaphore.acquire_owned().await; // ← BLOCKS HERE

// If all 5 permits in use, worker waits
// Jobs remain in database, not memory
```

This is **structural backpressure**—you cannot bypass it.

#### 2. Database Boundary (Secondary)

If database becomes slow or unavailable:

```rust
match acquire_next_job().await {
    Ok(Some(job)) => { /* process */ },
    Ok(None) => {
        // No jobs available - sleep
        sleep(Duration::from_secs(5)).await;
    },
    Err(e) => {
        // Database error - back off
        error!("DB error: {:?}", e);
        sleep(Duration::from_secs(1)).await;
    }
}
```

### Failure Scenarios

| Scenario | Behavior | Why It's Safe |
|----------|----------|---------------|
| **1000 jobs arrive instantly** | Workers acquire 5, spawn 5 tasks, wait for permits | Jobs 6-1000 remain in DB, not RAM |
| **Worker panics** | Permit auto-released via RAII, other workers continue | No cascading failure |
| **Slow job (30s instead of 3s)** | Throughput drops but system stable | Semaphore prevents overload |
| **Database connection timeout** | Worker sleeps 1s, retries | Exponential backoff (future work) |
| **API flood (1000 req/s)** | Connection pool limits API requests | DB connections reserved for workers |

> **Node.js equivalent**: See [README.md](./README.md#complete-nodejs-implementation) for Node.js implementation patterns.

---

## Configuration for Different Workloads

### Rule of Thumb

```
WORKERS ≈ SEMAPHORE / 2 to SEMAPHORE / 3
DB_CONNECTIONS ≥ WORKERS + SEMAPHORE + API_BUFFER
```

### Deployment Scenarios

#### 1. Development (Low Load)
```bash
NUM_WORKERS=2
MAX_CONCURRENT_JOBS=3
MAX_DB_CONNECTIONS=10
```
- Throughput: ~3 jobs/second
- Use case: Local testing

#### 2. Production Balanced (Recommended)
```bash
NUM_WORKERS=3
MAX_CONCURRENT_JOBS=5
MAX_DB_CONNECTIONS=15
```
- Throughput: ~5-10 jobs/second
- Use case: Standard production workload

#### 3. High Throughput
```bash
NUM_WORKERS=5
MAX_CONCURRENT_JOBS=10
MAX_DB_CONNECTIONS=25
```
- Throughput: ~10-20 jobs/second
- Use case: Burst traffic, high volume

#### 4. CPU-Bound Jobs
```bash
NUM_WORKERS=2
MAX_CONCURRENT_JOBS=4  # Match CPU cores
MAX_DB_CONNECTIONS=15
```
- Throughput: Limited by CPU
- Use case: Video encoding, data processing

#### 5. I/O-Bound Jobs
```bash
NUM_WORKERS=5
MAX_CONCURRENT_JOBS=20
MAX_DB_CONNECTIONS=35
```
- Throughput: ~20-40 jobs/second
- Use case: HTTP requests, file I/O

---

## Monitoring & Observability

### Key Metrics to Track

#### 1. Semaphore Saturation
```
semaphore_permits_available
semaphore_permits_in_use
```

**What it tells you:**
- Always at 0 permits → Increase `MAX_CONCURRENT_JOBS`
- Always at max permits → Jobs not arriving fast enough

#### 2. Worker Idle Rate
```
worker_jobs_acquired_per_second
worker_idle_time_percent
```

**What it tells you:**
- High idle time + jobs in queue → Database slow
- High idle time + no jobs → System healthy

#### 3. Connection Pool Usage
```
db_connections_active
db_connections_idle
db_connections_wait_time
```

**What it tells you:**
- Wait time > 0 → Increase `MAX_DB_CONNECTIONS`
- Always at max → Review query performance

#### 4. Job Latency
```
job_acquisition_latency_ms
job_processing_latency_ms
job_queue_wait_time_ms
```

**What it tells you:**
- Queue wait time increasing → Need more throughput
- Processing latency increasing → Optimize job logic

### Log Messages to Watch

```
✅ Good:
Worker 1 acquired job: id=42
Worker 1 got semaphore permit for job 42
Processing job 42 for 3 seconds
Completed job 42: status=success

⚠️ Warning:
Worker 1 found no jobs available, sleeping...
(Repeatedly → either no load or database issue)

❌ Error:
Worker 1 encountered database error: connection timeout
Failed to update job 42: connection pool exhausted
```

---

## What I Learned Building This

### 1. Resource Limits Are Hard Constraints

In Node.js, it's easy to write:

```javascript
await Promise.all(jobs.map(processJob)) // Unbounded!
```

Rust's semaphore forced me to think: **How many jobs can I safely process?**

This question applies to every production system, regardless of language.

### 2. Backpressure Belongs at Boundaries

Don't let queues grow unbounded in memory. Apply backpressure where resources are **acquired**, not where they're **consumed**.

In this system:
- Jobs acquired → database (durable)
- Permits acquired → semaphore (bounded)
- Never in RAM → OOM impossible

### 3. Connection Pools Are Explicit Costs

Formula-based sizing:
```
WORKERS + CONCURRENT_JOBS + API_BUFFER
```

I now apply this in Node.js projects explicitly.

### 4. Workers ≠ Concurrency

**Workers** = job acquisition rate
**Semaphore** = processing capacity

Separating these concerns makes the system easier to reason about and tune.

### 5. RAII for Correctness

Semaphore permits release automatically on:
- Normal completion
- Early return
- Panic

This pattern prevents resource leaks I've debugged in Node.js where manual cleanup failed.

---

## Rust-Specific Implementation Insights

This section highlights Rust-specific patterns that made this architecture reliable and performant.

### 1. Ownership Prevents Data Races at Compile Time

**The Pattern:**
```rust
pub async fn acquire_next_job(&self) -> Result<Option<JobRow>, sqlx::Error> {
    let mut tx = self.pool.begin().await?;

    let job = sqlx::query_as::<_, JobRow>(/* ... */)
        .fetch_optional(&mut *tx)
        .await?;

    if let Some(job) = job {
        sqlx::query(/* UPDATE to processing */)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(Some(job))
    } else {
        tx.rollback().await?;
        Ok(None)
    }
}
```

**Why This Matters:**
- `JobRow` is **moved** to the caller, not cloned or referenced
- No other code can access this job simultaneously
- Compiler enforces single ownership—no runtime checks needed
- Impossible to accidentally process the same job twice

### 2. `Arc<Semaphore>` for Lock-Free Concurrency Control

**The Pattern:**
```rust
let semaphore = Arc::new(Semaphore::new(5));

for worker_id in 1..=3 {
    let worker_semaphore = semaphore.clone(); // Arc refcount incremented
    tokio::spawn(async move {
        // worker_semaphore moved into task
    });
}
```

**Why This Matters:**
- `Arc` uses atomic operations, not locks
- `clone()` is cheap (just increments refcount)
- No mutex required—Tokio's `Semaphore` is internally synchronized
- When all `Arc` references dropped, semaphore deallocated automatically

### 3. RAII Ensures Resource Cleanup on Panic

**The Pattern:**
```rust
let permit = semaphore.acquire_owned().await?;

tokio::spawn(async move {
    process_job(job).await; // May panic
    drop(permit); // Runs even if panic occurs
});
```

**What Happens on Panic:**
```
1. Task panics inside tokio::spawn
2. Tokio catches panic (isolated to this task)
3. Stack unwinds
4. permit's Drop trait runs automatically
5. Semaphore permit released
6. Other workers continue normally
```

**Contrast with Node.js:**
- In Node.js: Must manually release in `finally` blocks
- If `finally` missed → resource leak
- Rust: Compiler guarantees `Drop` runs

### 4. Async Transaction Safety

**The Pattern:**
```rust
let mut tx = pool.begin().await?;  // Start transaction

// If this function returns early (via ?), tx is dropped
let job = query_as(/* ... */).fetch_optional(&mut *tx).await?;

// If we never call commit(), transaction auto-rollback on drop
tx.commit().await?;
```

**Why This Matters:**
- Forgetting to commit → automatic rollback (safe default)
- Early return → automatic rollback
- Panic → automatic rollback
- Impossible to leave database in inconsistent state

### 5. Type-State Pattern for Job Lifecycle

**Current Implementation:**
```rust
pub struct JobRow {
    pub id: i32,
    pub name: String,
    pub status: String, // "new" | "processing" | "success" | "failed"
}
```

**Production Enhancement (Type-State Pattern):**
```rust
// Phantom types ensure compile-time state tracking
pub struct Job<State> {
    id: i32,
    name: String,
    _state: PhantomData<State>,
}

pub struct New;
pub struct Processing;
pub struct Completed;

// Only new jobs can be acquired
impl Job<New> {
    pub fn acquire(self) -> Job<Processing> { /* ... */ }
}

// Only processing jobs can be completed
impl Job<Processing> {
    pub fn complete(self) -> Job<Completed> { /* ... */ }
}

// Compiler prevents: new_job.complete() ← won't compile!
```

This pattern would make invalid state transitions impossible at compile time.

### 6. Zero-Cost Abstraction: SQLx Compile-Time Verification

**At Compile Time:**
```bash
cargo build
# SQLx connects to DATABASE_URL
# Verifies SQL syntax
# Verifies column types match Rust types
# Fails compilation if schema mismatches
```

**Runtime Cost:** Zero overhead. Compiled code is identical to hand-written SQL parsing.

**Comparison:**
| Language | Query Verification | Runtime Cost |
|----------|-------------------|--------------|
| Node.js | Runtime only | Parsing + validation overhead |
| Python | Runtime only | Parsing + ORM overhead |
| Go | Runtime only | Reflection overhead |
| **Rust (SQLx)** | **Compile-time** | **Zero overhead** |

### 7. Fearless Concurrency: No Data Races Possible

**This code won't compile:**
```rust
let mut counter = 0;

for _ in 0..3 {
    tokio::spawn(async {
        counter += 1; // ERROR: cannot move out of captured variable
    });
}
```

**Compiler error:**
```
error[E0373]: closure may outlive the current function, but it borrows
variables which are owned by the current function
```

**Rust forces you to use proper synchronization:**
```rust
let counter = Arc::new(AtomicU32::new(0));

for _ in 0..3 {
    let c = counter.clone();
    tokio::spawn(async move {
        c.fetch_add(1, Ordering::SeqCst); // ✓ Safe
    });
}
```

**Why This Matters:** Data races are impossible in safe Rust. The compiler catches them before runtime.

### 8. Performance: No Garbage Collection Pauses

**Node.js under load:**
```
Job processing: 50ms
GC pause: 20-100ms (unpredictable)
Total: 70-150ms
```

**Rust under load:**
```
Job processing: 50ms
GC pause: 0ms (no GC)
Total: 50ms (predictable)
```

**Rust uses deterministic destruction:**
- Memory freed immediately when `Drop` runs
- No stop-the-world GC pauses
- Latency is consistent under load

---

## Conclusion

This worker architecture is **language-agnostic**. The patterns apply equally to Node.js, Python, Go, or any concurrent system.

Rust forced me to make these decisions explicit:
- How many workers?
- How many concurrent jobs?
- How many database connections?
- Where does backpressure apply?

I now ask these questions in **every** system I design, regardless of language.

**For Rust-focused work:** This project demonstrates production-ready patterns: ownership for correctness, RAII for safety, zero-cost abstractions for performance, and compile-time guarantees for reliability.

**For Node.js-focused work:** The system design principles translate directly—Rust simply enforced them at compile time rather than relying on discipline.

---

## Further Reading

- [PostgreSQL Advisory Locks](https://www.postgresql.org/docs/current/explicit-locking.html#ADVISORY-LOCKS)
- [Semaphores in Tokio](https://docs.rs/tokio/latest/tokio/sync/struct.Semaphore.html)
- [BullMQ Concurrency Patterns](https://docs.bullmq.io/guide/workers/concurrency)
- [Connection Pool Sizing (HikariCP Guide)](https://github.com/brettwooldridge/HikariCP/wiki/About-Pool-Sizing)

---

**Built as a learning exercise by a backend engineer exploring systems fundamentals.**

*For questions about Node.js parallels or production deployment strategies, feel free to reach out.*
