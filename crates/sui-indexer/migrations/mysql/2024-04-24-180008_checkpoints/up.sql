CREATE TABLE checkpoints
(
    sequence_number                     bigint       PRIMARY KEY,
    checkpoint_digest                   BLOB         NOT NULL,
    epoch                               BIGINT       NOT NULL,
    -- total transactions in the network at the end of this checkpoint (including itself)
    network_total_transactions          BIGINT       NOT NULL,
    previous_checkpoint_digest          BLOB,
    -- if this checkpoitn is the last checkpoint of an epoch
    end_of_epoch                        BOOLEAN      NOT NULL,
    -- array of TranscationDigest in bytes included in this checkpoint
    tx_digests                          JSON         NOT NULL,
    timestamp_ms                        BIGINT       NOT NULL,
    total_gas_cost                      BIGINT       NOT NULL,
    computation_cost                    BIGINT       NOT NULL,
    storage_cost                        BIGINT       NOT NULL,
    storage_rebate                      BIGINT       NOT NULL,
    non_refundable_storage_fee          BIGINT       NOT NULL,
    -- bcs serialized Vec<CheckpointCommitment> bytes
    checkpoint_commitments              MEDIUMBLOB   NOT NULL,
    -- bcs serialized AggregateAuthoritySignature bytes
    validator_signature                 BLOB         NOT NULL,
    -- bcs serialzied EndOfEpochData bytes, if the checkpoint marks end of an epoch
    end_of_epoch_data                   BLOB
);

CREATE INDEX checkpoints_epoch ON checkpoints (epoch, sequence_number);
CREATE INDEX checkpoints_digest ON checkpoints (checkpoint_digest(32));

CREATE TABLE pruner_cp_watermark (
    checkpoint_sequence_number  BIGINT       PRIMARY KEY,
    min_tx_sequence_number      BIGINT       NOT NULL,
    max_tx_sequence_number      BIGINT       NOT NULL
)
