CREATE TABLE checkpoints
(
    sequence_number                     BIGINT       PRIMARY KEY,
    checkpoint_digest                   BYTEA        NOT NULL,
    epoch                               BIGINT       NOT NULL,
    -- total transactions in the network at the end of this checkpoint (including itself)
    network_total_transactions          BIGINT       NOT NULL,
    previous_checkpoint_digest          BYTEA,
    -- if this checkpoitn is the last checkpoint of an epoch
    end_of_epoch                        boolean      NOT NULL,
    -- array of TranscationDigest in bytes included in this checkpoint
    tx_digests                          BYTEA[]      NOT NULL,
    timestamp_ms                        BIGINT       NOT NULL,
    total_gas_cost                      BIGINT       NOT NULL,
    computation_cost                    BIGINT       NOT NULL,
    storage_cost                        BIGINT       NOT NULL,
    storage_rebate                      BIGINT       NOT NULL,
    non_refundable_storage_fee          BIGINT       NOT NULL,
    -- bcs serialized Vec<CheckpointCommitment> bytes
    checkpoint_commitments              BYTEA        NOT NULL,
    -- bcs serialized AggregateAuthoritySignature bytes
    validator_signature                 BYTEA        NOT NULL,
    -- bcs serialzied EndOfEpochData bytes, if the checkpoint marks end of an epoch
    end_of_epoch_data                   BYTEA,
    min_tx_sequence_number              BIGINT,
    max_tx_sequence_number              BIGINT
);

CREATE INDEX checkpoints_epoch ON checkpoints (epoch, sequence_number);
CREATE INDEX checkpoints_digest ON checkpoints USING HASH (checkpoint_digest);

CREATE TABLE pruner_cp_watermark (
    checkpoint_sequence_number  BIGINT       PRIMARY KEY,
    min_tx_sequence_number      BIGINT       NOT NULL,
    max_tx_sequence_number      BIGINT       NOT NULL
)
