CREATE TABLE epochs
(
    epoch                           BIGINT      PRIMARY KEY,
    validators                      bytea[]     NOT NULL,
    first_checkpoint_id             BIGINT      NOT NULL,
    epoch_start_timestamp           BIGINT      NOT NULL,
    reference_gas_price             BIGINT      NOT NULL,
    protocol_version                BIGINT      NOT NULL,
    -- The following fields are nullable because they are filled in
    -- only at the end of an epoch.
    epoch_total_transactions        BIGINT,
    last_checkpoint_id              BIGINT,
    epoch_end_timestamp             BIGINT,
    -- The following fields are from SystemEpochInfoEvent emitted
    -- **after** advancing to the next epoch
    storage_fund_reinvestment       BIGINT,
    storage_charge                  BIGINT,
    storage_rebate                  BIGINT,
    storage_fund_balance            BIGINT,
    stake_subsidy_amount            BIGINT,
    total_gas_fees                  BIGINT,
    total_stake_rewards_distributed BIGINT,
    leftover_storage_fund_inflow    BIGINT,
    -- total stake after advancing to the next epoch
    new_total_stake                 BIGINT,
    -- bcs serialized Vec<EpochCommitment> bytes, found in last CheckpointSummary
    -- of the epoch
    epoch_commitments               bytea,
    -- They are here as part of EndofEpochData that would be returned in `get_epoch(s)`
    next_epoch_reference_gas_price  BIGINT,
    next_epoch_protocol_version     BIGINT
);
