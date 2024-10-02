CREATE TABLE IF NOT EXISTS chaindata.EVENT
(
    transaction_digest STRING        NOT NULL,
    event_index        INT64         NOT NULL,
    checkpoint         INT64         NOT NULL,
    epoch              INT64         NOT NULL,
    timestamp_ms       INT64         NOT NULL,
    sender             STRING        NOT NULL,
    package            STRING        NOT NULL,
    module             STRING        NOT NULL,
    event_type         STRING        NOT NULL,
    bcs                STRING        NOT NULL,
    event_json         JSON
)
PARTITION BY RANGE_BUCKET(epoch, GENERATE_ARRAY(0, 100000, 10))
CLUSTER BY transaction_digest, event_index