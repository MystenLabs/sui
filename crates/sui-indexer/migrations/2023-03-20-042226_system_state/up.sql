CREATE TABLE system_states
(
    epoch                              BIGINT PRIMARY KEY,
    protocol_version                   BIGINT   NOT NULL,
    system_state_version               BIGINT   NOT NULL,
    storage_fund                       BIGINT   NOT NULL,
    reference_gas_price                BIGINT   NOT NULL,
    safe_mode                          BOOLEAN  NOT NULL,
    epoch_start_timestamp_ms           BIGINT   NOT NULL,
    epoch_duration_ms                  BIGINT   NOT NULL,
    stake_subsidy_start_epoch          BIGINT   NOT NULL,
    stake_subsidy_epoch_counter        BIGINT   NOT NULL,
    stake_subsidy_balance              BIGINT   NOT NULL,
    stake_subsidy_current_epoch_amount BIGINT   NOT NULL,
    total_stake                        BIGINT   NOT NULL,
    pending_active_validators_id       TEXT     NOT NULL,
    pending_active_validators_size     BIGINT   NOT NULL,
    pending_removals                   BIGINT[] NOT NULL,
    staking_pool_mappings_id           TEXT     NOT NULL,
    staking_pool_mappings_size         BIGINT   NOT NULL,
    inactive_pools_id                  TEXT     NOT NULL,
    inactive_pools_size                BIGINT   NOT NULL,
    validator_candidates_id            TEXT     NOT NULL,
    validator_candidates_size          BIGINT   NOT NULL
);

CREATE TABLE validators
(
    epoch                            BIGINT NOT NULL,
    sui_address                      TEXT   NOT NULL,
    protocol_pubkey_bytes            bytea  NOT NULL,
    network_pubkey_bytes             bytea  NOT NULL,
    worker_pubkey_bytes              bytea  NOT NULL,
    proof_of_possession_bytes        bytea  NOT NULL,
    name                             TEXT   NOT NULL,
    description                      TEXT   NOT NULL,
    image_url                        TEXT   NOT NULL,
    project_url                      TEXT   NOT NULL,
    net_address                      TEXT   NOT NULL,
    p2p_address                      TEXT   NOT NULL,
    primary_address                  TEXT   NOT NULL,
    worker_address                   TEXT   NOT NULL,
    next_epoch_protocol_pubkey_bytes bytea,
    next_epoch_proof_of_possession   bytea,
    next_epoch_network_pubkey_bytes  bytea,
    next_epoch_worker_pubkey_bytes   bytea,
    next_epoch_net_address           TEXT,
    next_epoch_p2p_address           TEXT,
    next_epoch_primary_address       TEXT,
    next_epoch_worker_address        TEXT,
    voting_power                     BIGINT NOT NULL,
    operation_cap_id                 TEXT   NOT NULL,
    gas_price                        BIGINT NOT NULL,
    commission_rate                  BIGINT NOT NULL,
    next_epoch_stake                 BIGINT NOT NULL,
    next_epoch_gas_price             BIGINT NOT NULL,
    next_epoch_commission_rate       BIGINT NOT NULL,
    staking_pool_id                  TEXT   NOT NULL,
    staking_pool_activation_epoch    BIGINT,
    staking_pool_deactivation_epoch  BIGINT,
    staking_pool_sui_balance         BIGINT NOT NULL,
    rewards_pool                     BIGINT NOT NULL,
    pool_token_balance               BIGINT NOT NULL,
    pending_stake                    BIGINT NOT NULL,
    pending_total_sui_withdraw       BIGINT NOT NULL,
    pending_pool_token_withdraw      BIGINT NOT NULL,
    exchange_rates_id                TEXT   NOT NULL,
    exchange_rates_size              BIGINT NOT NULL,
    CONSTRAINT validators_pk PRIMARY KEY (epoch, sui_address)
);

CREATE TABLE at_risk_validators
(
    epoch       BIGINT NOT NULL,
    address     TEXT   NOT NULL,
    epoch_count BIGINT NOT NULL,
    reported_by TEXT[] NOT NULL,
    CONSTRAINT at_risk_validators_pk PRIMARY KEY (EPOCH, address)
);

-- NOTE: this assumes that over the past 100 checkpoints, there are at least 
-- 2 checkpoints with different timestamps.
-- This is an optimization to avoid fetching & calculating all checkpoints.
CREATE OR REPLACE VIEW real_time_tps AS 
WITH recent_checkpoints AS (
  SELECT
    sequence_number,
    total_successful_transactions,
    timestamp_ms
  FROM
    checkpoints
  ORDER BY
    timestamp_ms DESC
  LIMIT 100
),
diff_checkpoints AS (
  SELECT
    MAX(sequence_number) as sequence_number,
    SUM(total_successful_transactions) as total_successful_transactions,
    LAG(timestamp_ms) OVER (ORDER BY timestamp_ms DESC) - timestamp_ms AS time_diff
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
SELECT (SELECT recent_tps from real_time_tps)                                                       AS current_tps,
       (SELECT COALESCE(tps_30_days, 0) FROM epoch_network_metrics)                                 AS tps_30_days,
       (SELECT COUNT(1) FROM addresses)                                                             AS total_addresses,
       -- row estimation
       (SELECT reltuples AS estimate FROM pg_class WHERE relname = 'objects')::BIGINT               AS total_objects,
       (SELECT COUNT(1) FROM packages)                                                              AS total_packages,
       (SELECT MAX(epoch) FROM epochs)                                                              AS current_epoch,
       (SELECT MAX(sequence_number) FROM checkpoints)                                               AS current_checkpoint;
