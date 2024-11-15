CREATE TABLE IF NOT EXISTS kv_protocol_configs
(
    protocol_version            BIGINT        NOT NULL,
    config_name                 TEXT          NOT NULL,
    config_value                TEXT,
    PRIMARY KEY (protocol_version, config_name)
);
