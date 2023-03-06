CREATE TABLE move_calls (
    id BIGSERIAL PRIMARY KEY,
    transaction_digest VARCHAR(255) NOT NULL,
    checkpoint_sequence_number BIGINT NOT NULL,
    epoch BIGINT NOT NULL,
    sender TEXT NOT NULL,
    move_package TEXT NOT NULL,
    move_module TEXT NOT NULL,
    move_function TEXT NOT NULL
);

CREATE INDEX move_calls_transaction_digest ON move_calls (transaction_digest);
CREATE INDEX move_calls_checkpoint_sequence_number ON move_calls (checkpoint_sequence_number);
CREATE INDEX move_calls_epoch ON move_calls (epoch);
CREATE INDEX move_calls_sender ON move_calls (sender);
CREATE INDEX move_calls_move_package ON move_calls (move_package);
CREATE INDEX move_calls_move_module ON move_calls (move_module);
CREATE INDEX move_calls_move_function ON move_calls (move_function);
