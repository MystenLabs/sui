CREATE TABLE IF NOT EXISTS kv_feature_flags
(
    protocol_version            BIGINT        NOT NULL,
    flag_name                   TEXT          NOT NULL,
    flag_value                  BOOLEAN       NOT NULL,
    PRIMARY KEY (protocol_version, flag_name)
);
