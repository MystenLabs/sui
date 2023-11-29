CREATE TABLE move_calls (
    transaction_sequence_number BIGINT  NOT NULL,
    checkpoint_sequence_number  BIGINT  NOT NULL,
    epoch                       BIGINT  NOT NULL,
    move_package                BYTEA   NOT NULL,
    move_module                 TEXT    NOT NULL,
    move_function               TEXT    NOT NULL,
    PRIMARY KEY(transaction_sequence_number, move_package, move_module, move_function)
);
CREATE INDEX idx_move_calls_epoch_etc ON move_calls (epoch, move_package, move_module, move_function);

CREATE TABLE move_call_metrics (
    -- Diesel only supports table with a primary key.
    id                          BIGSERIAL   PRIMARY KEY,
    epoch                       BIGINT      NOT NULL,
    day                         BIGINT      NOT NULL,
    move_package                TEXT        NOT NULL,
    move_module                 TEXT        NOT NULL,
    move_function               TEXT        NOT NULL,
    count                       BIGINT      NOT NULL
);
CREATE INDEX move_call_metrics_epoch_day ON move_call_metrics (epoch, day);
