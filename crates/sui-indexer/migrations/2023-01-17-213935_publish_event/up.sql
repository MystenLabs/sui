-- TODO: this is a temp workaround for wave 2
-- remove when events throughput is high enough
CREATE TABLE publish_events (
    id BIGSERIAL PRIMARY KEY,
    -- below 2 are from Event ID, tx_digest and event_seq
    transaction_digest VARCHAR(255),
    event_sequence BIGINT NOT NULL,
    event_time TIMESTAMP,
    event_type VARCHAR NOT NULL,
    event_content VARCHAR NOT NULL,
    UNIQUE (transaction_digest, event_sequence)
);

CREATE INDEX publish_events_transaction_digest ON publish_events (transaction_digest);
CREATE INDEX publish_events_event_time ON publish_events (event_time);

CREATE TABLE publish_event_logs (
    id SERIAL PRIMARY KEY,
    next_cursor_tx_dig TEXT,
    next_cursor_event_seq BIGINT
);

INSERT INTO publish_event_logs (id, next_cursor_tx_dig, next_cursor_event_seq) VALUES
(1, NULL, NULL);
