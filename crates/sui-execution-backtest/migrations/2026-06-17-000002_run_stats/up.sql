-- Per-checkpoint replay denominators for a run, so divergence rates are queryable and comparable
-- across runs (`--task`). Populated unless `--no-stats` is set. Keyed by (task, checkpoint) for
-- idempotent re-commit.
CREATE TABLE run_stats (
    task                      TEXT    NOT NULL,
    epoch                     BIGINT  NOT NULL,
    checkpoint                BIGINT  NOT NULL,
    checked                   BIGINT  NOT NULL,
    executed                  BIGINT  NOT NULL,
    divergences               BIGINT  NOT NULL,
    reconstruction_errors     BIGINT  NOT NULL,
    coin_reservation_skipped  BIGINT  NOT NULL,
    execute_skipped           BIGINT  NOT NULL,
    gas_from_balance          BIGINT  NOT NULL,
    cancellation_excluded     BIGINT  NOT NULL,
    PRIMARY KEY (task, checkpoint)
);
