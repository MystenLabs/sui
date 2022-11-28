CREATE TABLE EVENTS (
    id BIGSERIAL PRIMARY KEY,
    transaction_digest VARCHAR(255),
    -- below 2 are from Event ID, tx_seq and event_seq
    transaction_sequence BIGINT NOT NULL,
    event_sequence BIGINT NOT NULL,
    event_time TIMESTAMP,
    event_type VARCHAR NOT NULL,
    event_content VARCHAR NOT NULL,
    UNIQUE (transaction_sequence, event_sequence)
);
