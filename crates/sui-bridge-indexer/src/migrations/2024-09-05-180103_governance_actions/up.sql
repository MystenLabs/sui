CREATE TABLE governance_actions
(
    id                          BIGSERIAL    PRIMARY KEY,
    txn_digest                  bytea        NOT NULL,
    sender_address              bytea        NOT NULL,
    timestamp_ms                BIGINT       NOT NULL,
    action                      text         NOT NULL,
    data                        JSONB        NOT NULL
);