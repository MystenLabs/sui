-- senders or recipients of transactions
CREATE TABLE addresses
(
    address                 BYTEA   PRIMARY KEY,
    first_appearance_tx     BIGINT  NOT NULL,
    first_appearance_time   BIGINT  NOT NULL,
    last_appearance_tx      BIGINT  NOT NULL,
    last_appearance_time    BIGINT  NOT NULL
);

-- senders of transactions
CREATE TABLE active_addresses
(
    address                 BYTEA   PRIMARY KEY,
    first_appearance_tx     BIGINT  NOT NULL,
    first_appearance_time   BIGINT  NOT NULL,
    last_appearance_tx      BIGINT  NOT NULL,
    last_appearance_time    BIGINT  NOT NULL
);
CREATE INDEX active_addresses_last_appearance_tx_idx ON active_addresses (last_appearance_tx);

CREATE TABLE address_metrics
(
    checkpoint                  BIGINT  PRIMARY KEY,
    epoch                       BIGINT  NOT NULL,
    timestamp_ms                BIGINT  NOT NULL,
    cumulative_addresses        BIGINT  NOT NULL,
    cumulative_active_addresses BIGINT  NOT NULL,
    daily_active_addresses      BIGINT  NOT NULL
);
CREATE INDEX address_metrics_epoch_idx ON address_metrics (epoch);
