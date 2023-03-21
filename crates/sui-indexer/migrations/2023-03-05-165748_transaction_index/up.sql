CREATE TABLE move_calls (
    id                          BIGSERIAL       PRIMARY KEY,
    transaction_digest          base58digest    NOT NULL,
    checkpoint_sequence_number  BIGINT          NOT NULL,
    epoch                       BIGINT          NOT NULL,
    sender                      address         NOT NULL,
    move_package                TEXT            NOT NULL,
    move_module                 TEXT            NOT NULL,
    move_function               TEXT            NOT NULL
);

CREATE INDEX move_calls_transaction_digest ON move_calls (transaction_digest);
CREATE INDEX move_calls_checkpoint_sequence_number ON move_calls (checkpoint_sequence_number);
CREATE INDEX move_calls_epoch ON move_calls (epoch);
CREATE INDEX move_calls_sender ON move_calls (sender);
CREATE INDEX move_calls_move_package ON move_calls (move_package);
CREATE INDEX move_calls_move_module ON move_calls (move_module);
CREATE INDEX move_calls_move_function ON move_calls (move_function);

CREATE TABLE recipients (
    id                          BIGSERIAL       PRIMARY KEY,
    transaction_digest          base58digest    NOT NULL,
    checkpoint_sequence_number  BIGINT          NOT NULL,
    epoch                       BIGINT          NOT NULL,
    recipient                   address         NOT NULL
);

CREATE INDEX recipients_transaction_digest ON recipients (transaction_digest);
CREATE INDEX recipients_checkpoint_sequence_number ON recipients (checkpoint_sequence_number);
CREATE INDEX recipients_epoch ON recipients (epoch);
CREATE INDEX recipients_recipient ON recipients (recipient);

CREATE TABLE input_objects (
    id                          BIGSERIAL       PRIMARY KEY,
    transaction_digest          base58digest    NOT NULL,
    checkpoint_sequence_number  BIGINT          NOT NULL,
    epoch                       BIGINT          NOT NULL,
    object_id                   address         NOT NULL,
    object_version              BIGINT
);

CREATE INDEX input_objects_transaction_digest ON input_objects (transaction_digest);
CREATE INDEX input_objects_checkpoint_sequence_number ON input_objects (checkpoint_sequence_number);
CREATE INDEX input_objects_epoch ON input_objects (epoch);
CREATE INDEX input_objects_object_id ON input_objects (object_id);
CREATE INDEX input_objects_object_version ON input_objects (object_version);
