CREATE TABLE IF NOT EXISTS chaindata.MOVE_CALL
(
    transaction_digest STRING NOT NULL,
    checkpoint         INT64  NOT NULL,
    epoch              INT64  NOT NULL,
    timestamp_ms       INT64  NOT NULL,
    package            STRING NOT NULL,
    module             STRING NOT NULL,
    function_          STRING NOT NULL
) PARTITION BY RANGE_BUCKET(epoch, GENERATE_ARRAY(0, 100000, 10))
CLUSTER BY transaction_digest