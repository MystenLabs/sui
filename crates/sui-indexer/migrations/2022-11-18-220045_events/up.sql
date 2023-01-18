CREATE TABLE EVENTS (
    id BIGSERIAL PRIMARY KEY,
    -- below 2 are from Event ID, tx_digest and event_seq
    transaction_digest VARCHAR(255),
    event_sequence BIGINT NOT NULL,
    event_time TIMESTAMP,
    event_type VARCHAR NOT NULL,
    event_content VARCHAR NOT NULL,
    UNIQUE (transaction_digest, event_sequence)
);
