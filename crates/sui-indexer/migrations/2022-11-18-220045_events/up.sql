CREATE TABLE EVENTS (
    id BIGSERIAL PRIMARY KEY,
    -- txn digest is bytes of 32, but if we do exactly 32,
    -- DB write will fail with value too long for type character varying(32).
    transaction_digest VARCHAR(64),
    -- below 2 are from Event ID, tx_seq and event_seq
    transaction_sequence BIGINT NOT NULL,
    event_sequence BIGINT NOT NULL,
    event_time TIMESTAMP,
    event_type VARCHAR NOT NULL,
    event_content VARCHAR NOT NULL
);
