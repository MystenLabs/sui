CREATE TABLE watermarks
(
    -- The table name of group of tables governed by this watermark, i.e `epochs`, `checkpoints`,
    -- `transactions`. For example, `transactions` governs the `transactions` table and associated
    -- lookup tables.
    entity                      TEXT          NOT NULL,
    -- Inclusive upper bound epoch this entity has data for. Committer updates this field. Pruner
    -- uses this field for per-entity epoch-level retention, and is mostly useful for pruning
    -- unpartitioned tables.
    epoch_hi                    BIGINT        NOT NULL,
    -- Inclusive lower bound epoch this entity has data for. Pruner updates this field, and uses
    -- this field in tandem with `epoch_hi` for per-entity epoch-level retention. This is mostly
    -- useful for pruning unpartitioned tables.
    epoch_lo                    BIGINT        NOT NULL,
    -- Inclusive upper bound checkpoint this entity has data for. Committer updates this field. All
    -- data of this entity in the checkpoint must be persisted before advancing this watermark. The
    -- committer or ingestion task refers to this on disaster recovery.
    checkpoint_hi               BIGINT        NOT NULL,
    -- Inclusive high watermark that the committer advances. For `checkpoints`, this represents the
    -- checkpoint sequence number, for `transactions`, the transaction sequence number, etc.
    hi                          BIGINT        NOT NULL,
    -- Inclusive low watermark that the pruner advances. Data before this watermark is considered
    -- pruned by a reader.
    lo                          BIGINT        NOT NULL,
    -- Updated using the database's current timestamp when the pruner sees that some data needs to
    -- be dropped. The pruner uses this column to determine whether to prune or wait long enough
    -- that all in-flight reads complete or timeout before it acts on an updated watermark.
    timestamp_ms                BIGINT        NOT NULL,
    -- Pruner updates this, and uses this when recovering from a crash to determine where to
    -- continue pruning. Data at and below `pruned_lo` is considered pruned by the pruner. Has the
    -- same unit as `lo`.
    pruned_lo                   BIGINT,
    PRIMARY KEY (entity)
);
