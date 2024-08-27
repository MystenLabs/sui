CREATE TABLE checkpoints
(
    sequence_number                     bigint       PRIMARY KEY,
    -- bcs serialized CertifiedCheckpointSummary bytes
    certified_checkpoint_summary        bytea        NOT NULL,
    -- bcs serialized CheckpointContents bytes
    checkpoint_contents                 bytea        NOT NULL
);
