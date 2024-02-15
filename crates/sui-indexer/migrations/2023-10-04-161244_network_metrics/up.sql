CREATE TABLE epoch_peak_tps
(
    epoch           BIGINT  PRIMARY KEY,   
    peak_tps        FLOAT8  NOT NULL,
    peak_tps_30d    FLOAT8  NOT NULL
);

CREATE OR REPLACE VIEW real_time_tps AS 
WITH recent_checkpoints AS (
  SELECT
    checkpoint_sequence_number as sequence_number,
    total_successful_transactions,
    timestamp_ms
  FROM
    tx_count_metrics
  ORDER BY
    timestamp_ms DESC
  LIMIT 100
),
diff_checkpoints AS (
  SELECT
    MAX(sequence_number) as sequence_number,
    SUM(total_successful_transactions) as total_successful_transactions,
    timestamp_ms - LAG(timestamp_ms) OVER (ORDER BY timestamp_ms) AS time_diff
  FROM
    recent_checkpoints
  GROUP BY
    timestamp_ms
)
SELECT
  (total_successful_transactions * 1000.0 / time_diff)::float8 as recent_tps
FROM
  diff_checkpoints
WHERE 
  time_diff IS NOT NULL
ORDER BY sequence_number DESC LIMIT 1;

CREATE OR REPLACE VIEW network_metrics AS 
SELECT  (SELECT recent_tps from real_time_tps)                                                          AS current_tps,
        (SELECT COALESCE(peak_tps_30d, 0) FROM epoch_peak_tps ORDER BY epoch DESC LIMIT 1)              AS tps_30_days,
        (SELECT reltuples AS estimate FROM pg_class WHERE relname = 'addresses')::BIGINT                AS total_addresses,
        (SELECT reltuples AS estimate FROM pg_class WHERE relname = 'objects')::BIGINT                  AS total_objects,
        (SELECT reltuples AS estimate FROM pg_class WHERE relname = 'packages')::BIGINT                 AS total_packages,
        (SELECT MAX(epoch) FROM epochs)                                                                 AS current_epoch,
        (SELECT MAX(sequence_number) FROM checkpoints)                                                  AS current_checkpoint;
