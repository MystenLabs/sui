CREATE TABLE sui_error_transactions
(
    txn_digest                  bytea        PRIMARY KEY,
    sender_address              bytea        NOT NULL,
    timestamp_ms                BIGINT       NOT NULL,
    failure_status              text         NOT NULL,
    cmd_idx                     BIGINT
);