-- Information related to an epoch that is available when it starts
CREATE TABLE IF NOT EXISTS kv_epoch_starts
(
    epoch                       BIGINT        PRIMARY KEY,
    protocol_version            BIGINT        NOT NULL,

    -- Inclusive checkpoint lowerbound of the epoch.
    cp_lo                       BIGINT        NOT NULL,
    -- The timestamp that the epoch starts at. This is always extracted from
    -- the system state object.
    start_timestamp_ms          BIGINT        NOT NULL,
    -- The reference gas price that will be used for the rest of the epoch.
    reference_gas_price         BIGINT        NOT NULL,
    -- BCS serialized SystemState.
    system_state                BYTEA         NOT NULL
);

-- Information related to an epoch that is available when it ends (after the
-- epoch advancement to the next epoch)
CREATE TABLE IF NOT EXISTS kv_epoch_ends
(
    epoch                       BIGINT        PRIMARY KEY,

    -- Exclusive checkpoint upperbound of the epoch.
    cp_hi                       BIGINT        NOT NULL,
    -- Exclusive transaction upperbound of the epoch.
    tx_hi                       BIGINT        NOT NULL,

    -- The epoch ends at the timestamp of its last checkpoint.
    end_timestamp_ms            BIGINT        NOT NULL,

    -- Whether the epoch advancement at the end of this epoch entered safe
    -- mode.
    safe_mode                   BOOLEAN       NOT NULL,

    -- Staking information after advancement to the next epoch. These fields
    -- are extracted from the `SystemEpochInfoEvent` emitted during epoch
    -- advancement. If the epoch advancement entered safe mode, these fields
    -- will all be NULL (because a safe mode advance epoch does not emit this
    -- event).
    total_stake                 BIGINT,
    storage_fund_balance        BIGINT,
    storage_fund_reinvestment   BIGINT,
    storage_charge              BIGINT,
    storage_rebate              BIGINT,
    stake_subsidy_amount        BIGINT,
    total_gas_fees              BIGINT,
    total_stake_rewards_distributed
                                BIGINT,
    leftover_storage_fund_inflow
                                BIGINT,

    -- BCS serialized `Vec<EpochCommitment>` bytes, found in last
    -- `CheckpointSummary` of the epoch.
    epoch_commitments           BYTEA         NOT NULL
);
