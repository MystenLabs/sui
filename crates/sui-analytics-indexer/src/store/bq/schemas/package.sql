CREATE TABLE IF NOT EXISTS chaindata.MOVE_PACKAGE
(
    package_id         STRING        NOT NULL,
    checkpoint         INT64         NOT NULL,
    epoch              INT64         NOT NULL,
    timestamp_ms       INT64         NOT NULL,
    bcs                STRING        NOT NULL,
    transaction_digest STRING,
    package_version INT64,
    original_package_id STRING
)
PARTITION BY RANGE_BUCKET(epoch, GENERATE_ARRAY(0, 100000, 10))
CLUSTER BY package_id