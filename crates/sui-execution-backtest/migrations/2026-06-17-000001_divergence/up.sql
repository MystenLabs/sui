-- A transaction whose re-execution under the current rules disagrees with its on-chain status.
-- Keyed by (task, tx_digest) so re-committing a checkpoint after a crash is idempotent; `task` is
-- the run discriminator (see the `--task` flag) so runs under different execution rules don't
-- collide.
CREATE TABLE divergence (
    task                     TEXT    NOT NULL,
    epoch                    BIGINT  NOT NULL,
    checkpoint               BIGINT  NOT NULL,
    tx_digest                TEXT    NOT NULL,
    -- On-chain outcome.
    original_status          TEXT    NOT NULL,
    original_failure_kind    TEXT,
    -- Recomputed outcome.
    recomputed_status        TEXT    NOT NULL,
    recomputed_error_kind    TEXT,
    recomputed_error_detail  TEXT,
    -- Triage signals: how much of the read set our reconstructed store was missing / disagreed on.
    -- Nonzero values point at a reconstruction gap rather than a genuine execution divergence.
    missing_modified         BIGINT  NOT NULL,
    missing_loaded           BIGINT  NOT NULL,
    missing_consensus        BIGINT  NOT NULL,
    digest_mismatches        BIGINT  NOT NULL,
    PRIMARY KEY (task, tx_digest)
);

CREATE INDEX divergence_task_recomputed_error_kind
    ON divergence (task, recomputed_error_kind);
