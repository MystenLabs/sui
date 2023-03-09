CREATE TABLE events (
    id BIGSERIAL PRIMARY KEY,
    transaction_digest VARCHAR(255) NOT NULL,
    event_sequence BIGINT NOT NULL,
    event_time TIMESTAMP,
    event_type VARCHAR NOT NULL,
    event_content VARCHAR NOT NULL
);

CREATE INDEX events_transaction_digest ON events (transaction_digest);
CREATE INDEX events_event_time ON events (event_time);
