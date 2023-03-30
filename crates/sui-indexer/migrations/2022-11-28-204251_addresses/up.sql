CREATE TABLE addresses
(
    account_address       address       PRIMARY KEY,
    first_appearance_tx   base58digest  NOT NULL,
    first_appearance_time BIGINT        NOT NULL
);
CREATE INDEX addresses_account_address ON addresses (account_address);
