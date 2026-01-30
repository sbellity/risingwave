# Cold-tier table sharding (design spike)

> Status: design spike / exploration
>
> Goal: Enable RisingWave to support **100K–1M materialized views** (CDP segment membership) with mixed refresh-lag tiers (≈5s / 5min / 1h+) by reducing **metadata + commit overhead** from Hummock and barrier coordination.
>
> This doc focuses on **table consolidation** for cold tiers (shared physical tables) and how it composes with **sparse commits**.

## Problem

At massive MV counts, the bottlenecks are typically:

- **Table explosion**: each MV creates multiple internal state tables → millions of tables.
  - Meta memory grows with table metadata.
  - Hummock commit bookkeeping and compaction scheduling overhead grows with table count.
- **Commit cost scaling**: even if data is sparse, if commits enumerate many tables, `commit_epoch` becomes O(#tables) and dominates.

For CDP segments, many segments are cold (1h/6h) and/or updated sparsely. We want the system cost to be proportional to:

- tables that **changed** (dirty), and
- groups that are currently being refreshed/checkpointed,

not to total MV/table count.

## High-level approach

1) **Sparse commits**: commit only dirty tables/shards for a checkpoint (already aligns with Hummock’s `CommitEpochInfo.tables_to_commit`).
2) **Table consolidation**: reduce the number of *physical* state tables by mapping many *logical* tables (per MV/operator) into a bounded set of physical shard tables.

This doc focuses on (2).

## Terminology

- **Logical table id**: the “table id” used by query/MV/job semantics (what users and meta track today).
- **Physical table id**: an underlying Hummock state table used for storage and compaction.
- **Shard**: one physical table that stores many logical tables’ data.

## Proposed key layout

Current Hummock key layout (simplified):

```
<table_id> | <vnode> | <user_key>
```

Proposed sharded layout:

```
<physical_shard_table_id> | <vnode> | <logical_table_id> | <user_key>
```

Properties:
- Keeps distribution via vnode.
- Adds `logical_table_id` as an extra prefix so data remains logically separable.
- Bounds the number of physical tables (compaction groups scale with shards, not logical tables).

## How sharding is chosen

A simple deterministic mapping:

```
physical_shard = hash(logical_table_id) % SHARD_COUNT
physical_shard_table_id = base_table_id_for_family + physical_shard
```

Where:
- `SHARD_COUNT` is a fixed config (e.g., 1024 or 4096).
- `base_table_id_for_family` partitions between different families (e.g., MV state vs arrangement vs join state).

## State-table “families”

Not all state tables are equal. We likely shard only **cold-tier** and/or specific state-table families first.

Examples of families:
- materialize state
- arrangement state
- hash join state
- aggregation state

A minimal prototype should pick one family with clear semantics and measurable overhead.

## Interaction with compaction

Compaction currently reasons about tables and compaction groups. With sharding:

- Compaction works on physical shard tables.
- Per-logical-table stats need to be derived from embedded `logical_table_id` in keys OR maintained separately.

Expected changes:
- Table stats aggregation must understand that a single SST contains multiple logical tables.
- We must ensure compaction filters can still “split by table ids” efficiently.

### Key risk

Existing code uses per-table identifiers for:
- table-level stats
- change logs / watermarks
- GC / truncation logic

Sharding means these are no longer 1:1 with physical tables.

## Interaction with GC / retention

For CDP segments, cold tiers often have longer retention windows.

With sharding:
- GC still operates at physical-table granularity.
- Logical-table-specific retention (e.g., MV log store truncation) requires the ability to delete/compact away ranges for a specific `logical_table_id` prefix.

Potential approach:
- Use prefix tombstones / range tombstones keyed by `(logical_table_id, ...)` inside each shard.

## Migration / Backcompat strategy

A safe rollout plan:

1) Add feature flag to enable sharded tables for a specific family.
2) New MVs created under the flag use sharded physical tables.
3) Existing MVs remain unsharded.
4) Optionally add a migration tool later (copy state into shard tables) if worth it.

## How to validate (simulation harness)

Use `src/tests/simulation/` to validate:

1) Commit payload scale:
   - With many logical tables, ensure `tables_to_commit` stays near dirty set.
2) Compaction load:
   - Ensure number of compaction tasks does not explode with logical table count.
3) Latency under tiered refresh:
   - Hot tier keeps low-lag behavior.
   - Cold tier stays cheap.

## Open questions

- Which family is best for first sharding prototype?
- How does `split_sst_with_table_ids` and related code behave when SSTs include many logical ids?
- Can we make per-logical-table stats optional for cold tier?

## Next steps

- Implement prototype behind flag for one internal table family.
- Extend simulation tests to generate many logical tables and sparse writes.
- Measure commit_epoch and compaction scheduling behavior.
