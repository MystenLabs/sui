CREATE TABLE transaction_logs (
    id SERIAL PRIMARY KEY,
    next_cursor_tx_digest TEXT
);

INSERT INTO transaction_logs (id, next_cursor_tx_digest) VALUES
(1, NULL);
