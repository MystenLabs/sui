CREATE TABLE IF NOT EXISTS chaindata.WRAPPED_STRUCT
(
    object_id              STRING,
    root_object_id         STRING        NOT NULL,
    root_object_version    INT64         NOT NULL,
    checkpoint             INT64         NOT NULL,
    epoch                  INT64         NOT NULL,
    timestamp_ms           INT64         NOT NULL,
    json_path              STRING,
    struct_tag             STRING
)
    PARTITION BY RANGE_BUCKET(epoch, GENERATE_ARRAY(0, 100000, 10))
CLUSTER BY root_object_id, root_object_version, json_path