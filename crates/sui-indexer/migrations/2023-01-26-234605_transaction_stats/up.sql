CREATE TABLE transaction_stats (
    id BIGSERIAL PRIMARY KEY,
    computation_time TIMESTAMP NOT NULL,
    start_txn_time TIMESTAMP NOT NULL,
    end_txn_time TIMESTAMP NOT NULL,
    tps REAL NOT NULL
);

CREATE INDEX transaction_stats_computation_time ON transaction_stats (computation_time);
CREATE INDEX transaction_stats_start_txn_time ON transaction_stats (start_txn_time);
CREATE INDEX transaction_stats_end_txn_time ON transaction_stats (end_txn_time);
