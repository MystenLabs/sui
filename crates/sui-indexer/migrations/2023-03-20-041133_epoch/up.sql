CREATE TABLE epochs
(
    epoch                           BIGINT PRIMARY KEY,
    first_checkpoint_id             BIGINT NOT NULL,
    last_checkpoint_id              BIGINT,
    epoch_start_timestamp           BIGINT NOT NULL,
    epoch_end_timestamp             BIGINT,
    epoch_total_transactions        BIGINT NOT NULL,

    protocol_version                BIGINT,
    reference_gas_price             BIGINT,
    total_stake                     BIGINT,
    storage_fund_reinvestment       BIGINT,
    storage_charge                  BIGINT,
    storage_rebate                  BIGINT,
    storage_fund_balance            BIGINT,
    stake_subsidy_amount            BIGINT,
    total_gas_fees                  BIGINT,
    total_stake_rewards_distributed BIGINT,
    leftover_storage_fund_inflow    BIGINT
);
CREATE INDEX epochs_start_index ON epochs (epoch_start_timestamp ASC);
CREATE INDEX epochs_end_index ON epochs (epoch_end_timestamp ASC NULLS LAST);
