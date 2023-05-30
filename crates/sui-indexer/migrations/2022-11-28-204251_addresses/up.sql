CREATE TABLE addresses
(
    account_address       address       PRIMARY KEY,
    first_appearance_tx   base58digest  NOT NULL,
    first_appearance_time BIGINT        NOT NULL,
    last_appearance_tx    base58digest  NOT NULL,
    last_appearance_time  BIGINT        NOT NULL
);

CREATE TABLE active_addresses
(
    account_address       address       PRIMARY KEY,
    first_appearance_tx   base58digest  NOT NULL,
    first_appearance_time BIGINT        NOT NULL,
    last_appearance_tx    base58digest  NOT NULL,
    last_appearance_time  BIGINT        NOT NULL
);
CREATE INDEX active_addresses_last_appearance_time ON active_addresses (last_appearance_time);

CREATE TABLE address_stats
(
    checkpoint                      BIGINT  PRIMARY KEY,
    epoch                           BIGINT  NOT NULL,
    timestamp_ms                    BIGINT  NOT NULL,
    cumulative_addresses            BIGINT  NOT NULL,
    cumulative_active_addresses     BIGINT  NOT NULL,
    daily_active_addresses          BIGINT  NOT NULL
);
