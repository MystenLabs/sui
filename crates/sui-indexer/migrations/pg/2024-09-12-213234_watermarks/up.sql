CREATE TABLE watermarks
(
    -- The table governed by this watermark, i.e `epochs`, `checkpoints`, `transactions`.
    entity                      TEXT          NOT NULL,
    -- Inclusive upper epoch bound for this entity's data. Committer updates this field. Pruner uses
    -- this to determine if pruning is necessary based on the retention policy.
    epoch_hi_inclusive          BIGINT        NOT NULL,
    -- Inclusive lower epoch bound for this entity's data. Pruner updates this field when the epoch range exceeds the retention policy.
    epoch_lo                    BIGINT        NOT NULL,
    -- Inclusive upper checkpoint bound for this entity's data. Committer updates this field. All
    -- data of this entity in the checkpoint must be persisted before advancing this watermark. The
    -- committer refers to this on disaster recovery to resume writing.
    checkpoint_hi_inclusive     BIGINT        NOT NULL,
    -- Inclusive upper transaction sequence number bound for this entity's data. Committer updates
    -- this field.
    tx_hi_inclusive             BIGINT        NOT NULL,
    -- Inclusive low watermark that the pruner advances. Corresponds to the epoch id, checkpoint
    -- sequence number, or tx sequence number depending on the entity. Data before this watermark is
    -- considered pruned by a reader. The underlying data may still exist in the db instance.
    reader_lo                   BIGINT        NOT NULL,
    -- Updated using the database's current timestamp when the pruner sees that some data needs to
    -- be dropped. The pruner uses this column to determine whether to prune or wait long enough
    -- that all in-flight reads complete or timeout before it acts on an updated watermark.
    timestamp_ms                BIGINT        NOT NULL,
    -- Updated and used by the pruner. Data up to and excluding this watermark can be immediately
    -- dropped. Data between this and `reader_lo` can be pruned after a delay.
    pruner_lo                   BIGINT,
    PRIMARY KEY (entity)
);
