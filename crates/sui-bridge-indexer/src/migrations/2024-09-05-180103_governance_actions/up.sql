CREATE TABLE governance_actions
(
    txn_digest                  bytea        PRIMARY KEY,
    sender_address              bytea        NOT NULL,
    timestamp_ms                BIGINT       NOT NULL,
    action                      text         NOT NULL
);