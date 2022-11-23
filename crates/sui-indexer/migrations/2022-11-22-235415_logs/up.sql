CREATE TABLE event_logs (
    id SERIAL PRIMARY KEY,
    next_cursor_tx_seq BIGINT,
    next_cursor_event_seq BIGINT
);

CREATE TABLE transaction_logs (
    id SERIAL PRIMARY KEY,
    next_cursor_tx_digest TEXT
);

INSERT INTO event_logs (id, next_cursor_tx_seq, next_cursor_event_seq) VALUES
(1, NULL, NULL);

INSERT INTO transaction_logs (id, next_cursor_tx_digest) VALUES
(1, NULL);
