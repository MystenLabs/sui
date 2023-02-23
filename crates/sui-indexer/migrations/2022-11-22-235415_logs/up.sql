CREATE TABLE transaction_logs (
    id SERIAL PRIMARY KEY,
    next_checkpoint_sequence_number BIGINT NOT NULL
);

INSERT INTO transaction_logs (id, next_checkpoint_sequence_number) 
VALUES (1, 0);
