CREATE TABLE move_calls (
    -- Diesel only supports table with a primary key.
    id                          BIGSERIAL PRIMARY KEY,
    transaction_sequence_number BIGINT  NOT NULL,
    checkpoint_sequence_number  BIGINT  NOT NULL,
    epoch                       BIGINT  NOT NULL,
    move_package                TEXT    NOT NULL,
    move_module                 TEXT    NOT NULL,
    move_function               TEXT    NOT NULL
);
CREATE INDEX move_calls_epoch ON move_calls (epoch);

CREATE TABLE move_call_metrics (
    -- Diesel only supports table with a primary key.
    id              BIGSERIAL PRIMARY KEY,
    epoch           BIGINT    NOT NULL,
    day             BIGINT    NOT NULL,
    move_package    TEXT      NOT NULL,
    move_module     TEXT      NOT NULL,
    move_function   TEXT      NOT NULL,
    count           BIGINT    NOT NULL
);
CREATE INDEX move_calls_metics_epoch ON move_call_metrics (epoch);
