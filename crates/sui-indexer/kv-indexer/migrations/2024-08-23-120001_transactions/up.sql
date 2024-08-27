CREATE TABLE transactions (
    transaction_digest          bytea        NOT NULL,
    -- bcs serialized TransactionData bytes
    transaction_data            bytea        NOT NULL,
    -- bcs serialized TransactionEffects bytes
    effects                     bytea        NOT NULL,
    -- array of bcs serialized StoredEvent bytes
    events                      bytea[]      NOT NULL,
    checkpoint_sequence_number  bigint       NOT NULL,
    PRIMARY KEY (transaction_digest)
);
