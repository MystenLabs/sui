CREATE TABLE IF NOT EXISTS chaindata.OBJECT
(
    object_id              STRING        NOT NULL,
    version                INT64         NOT NULL,
    digest                 STRING        NOT NULL,
    type_                  STRING,
    checkpoint             INT64         NOT NULL,
    epoch                  INT64         NOT NULL,
    timestamp_ms           INT64         NOT NULL,
    owner_type             STRING        NOT NULL,
    owner_address          STRING,
    object_status          STRING        NOT NULL,
    initial_shared_version INT64,
    previous_transaction   STRING        NOT NULL,
    has_public_transfer    BOOL          NOT NULL,
    storage_rebate         NUMERIC(20, 0)         NOT NULL,
    bcs                    STRING        NOT NULL,
    coin_type              STRING,
    coin_balance           NUMERIC(20, 0),
    struct_tag             STRING,
    object_json            JSON
)
PARTITION BY RANGE_BUCKET(epoch, GENERATE_ARRAY(0, 100000, 10))
CLUSTER BY object_id, version