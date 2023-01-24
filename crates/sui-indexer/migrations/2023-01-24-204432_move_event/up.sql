CREATE TABLE move_events (
    id BIGSERIAL PRIMARY KEY,
    transaction_digest VARCHAR(255),
    event_sequence BIGINT NOT NULL,
    event_time TIMESTAMP,
    event_type VARCHAR NOT NULL,
    event_content VARCHAR NOT NULL,
    UNIQUE (transaction_digest, event_sequence)
);


CREATE INDEX move_events_transaction_digest ON move_events (transaction_digest);
CREATE INDEX move_events_event_time ON move_events (event_time);

CREATE TABLE move_event_logs (
    id SERIAL PRIMARY KEY,
    next_cursor_tx_dig TEXT,
    next_cursor_event_seq BIGINT
);

INSERT INTO move_event_logs (id, next_cursor_tx_dig, next_cursor_event_seq) VALUES
(1, NULL, NULL);
