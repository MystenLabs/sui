CREATE TABLE system_states
(
    epoch                              BIGINT PRIMARY KEY,
    protocol_version                   BIGINT   NOT NULL,
    system_state_version               BIGINT   NOT NULL,
    storage_fund                       BIGINT   NOT NULL,
    reference_gas_price                BIGINT   NOT NULL,
    safe_mode                          TINYINT(1)  NOT NULL,
    epoch_start_timestamp_ms           BIGINT   NOT NULL,
    epoch_duration_ms                  BIGINT   NOT NULL,
    stake_subsidy_start_epoch          BIGINT   NOT NULL,
    stake_subsidy_epoch_counter        BIGINT   NOT NULL,
    stake_subsidy_balance              BIGINT   NOT NULL,
    stake_subsidy_current_epoch_amount BIGINT   NOT NULL,
    total_stake                        BIGINT   NOT NULL,
    pending_active_validators_id       TEXT     NOT NULL,
    pending_active_validators_size     BIGINT   NOT NULL,
    pending_removals                   JSON     NOT NULL,
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
    sui_address                      VARCHAR(66)   NOT NULL,
    protocol_pubkey_bytes            BLOB  NOT NULL,
    network_pubkey_bytes             BLOB  NOT NULL,
    worker_pubkey_bytes              BLOB  NOT NULL,
    proof_of_possession_bytes        BLOB  NOT NULL,
    name                             TEXT   NOT NULL,
    description                      TEXT   NOT NULL,
    image_url                        TEXT   NOT NULL,
    project_url                      TEXT   NOT NULL,
    net_address                      TEXT   NOT NULL,
    p2p_address                      TEXT   NOT NULL,
    primary_address                  TEXT   NOT NULL,
    worker_address                   TEXT   NOT NULL,
    next_epoch_protocol_pubkey_bytes BLOB,
    next_epoch_proof_of_possession   BLOB,
    next_epoch_network_pubkey_bytes  BLOB,
    next_epoch_worker_pubkey_bytes   BLOB,
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
    address     VARCHAR(66)   NOT NULL,
    epoch_count BIGINT NOT NULL,
    reported_by JSON NOT NULL,
    CONSTRAINT at_risk_validators_pk PRIMARY KEY (EPOCH, address)
);

-- TODO(gegaowp): handle network_metrics view with a table in analytical pipelines.
-- CREATE OR REPLACE VIEW network_metrics AS
-- SELECT 
--     (
--         SELECT COALESCE(SUM(
--             CASE
--                 WHEN execution_success = 1 THEN transaction_count
--                 ELSE 1
--             END
--         ) / 10, 0) 
--         FROM transactions
--         WHERE timestamp_ms > 
--             (SELECT timestamp_ms FROM checkpoints ORDER BY sequence_number DESC LIMIT 1) - 10000
--     ) AS current_tps,
--     (
--         SELECT COALESCE(tps_30_days, 0) 
--         FROM epoch_network_metrics
--     ) AS tps_30_days,
--     (
--         SELECT COUNT(*) 
--         FROM addresses
--     ) AS total_addresses,
--     (
--         -- For MySQL, there's no direct equivalent for the row estimation feature in PostgreSQL
--         SELECT COUNT(*) 
--         FROM objects
--     ) AS total_objects,
--     (
--         SELECT COUNT(*) 
--         FROM packages
--     ) AS total_packages,
--     (
--         SELECT MAX(epoch) 
--         FROM epochs
--     ) AS current_epoch,
--     (
--         SELECT MAX(sequence_number) 
--         FROM checkpoints
--     ) AS current_checkpoint;
