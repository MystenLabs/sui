CREATE TABLE IF NOT EXISTS chaindata.TRANSACTION_OBJECT
(
    object_id          STRING        NOT NULL,
    version            INT64,
    transaction_digest STRING        NOT NULL,
    checkpoint         INT64         NOT NULL,
    epoch              INT64         NOT NULL,
    timestamp_ms       INT64         NOT NULL,
    input_kind         STRING,
    object_status      STRING
)
PARTITION BY RANGE_BUCKET(epoch, GENERATE_ARRAY(0, 100000, 10))
CLUSTER BY transaction_digest, object_id, version