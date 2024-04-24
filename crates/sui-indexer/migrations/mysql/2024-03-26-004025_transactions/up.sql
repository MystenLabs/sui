CREATE TABLE transactions (
                              tx_sequence_number          BIGINT       NOT NULL,
                              transaction_digest          BLOB        NOT NULL,
    -- bcs serialized SenderSignedData bytes
                              raw_transaction             BLOB        NOT NULL,
    -- bcs serialized TransactionEffects bytes
                              raw_effects                 BLOB        NOT NULL,
                              checkpoint_sequence_number  BIGINT       NOT NULL,
                              timestamp_ms                BIGINT       NOT NULL,
    -- array of bcs serialized IndexedObjectChange bytes
                              object_changes              JSON      NOT NULL,
    -- array of bcs serialized BalanceChange bytes
                              balance_changes             JSON      NOT NULL,
    -- array of bcs serialized StoredEvent bytes
                              events                      JSON      NOT NULL,
    -- SystemTransaction/ProgrammableTransaction. See types.rs
                              transaction_kind            smallint     NOT NULL,
    -- number of successful commands in this transaction, bound by number of command
    -- in a programmaable transaction.
                              success_command_count       smallint     NOT NULL,
                              CONSTRAINT transactions_pkey PRIMARY KEY (tx_sequence_number, checkpoint_sequence_number)
) PARTITION BY RANGE (checkpoint_sequence_number) (
    PARTITION p0 VALUES LESS THAN MAXVALUE
);

CREATE INDEX transactions_transaction_digest ON transactions (transaction_digest(255));
CREATE INDEX transactions_checkpoint_sequence_number ON transactions (checkpoint_sequence_number);
-- only create index for system transactions (0). See types.rs
CREATE INDEX transactions_transaction_kind ON transactions (transaction_kind);
