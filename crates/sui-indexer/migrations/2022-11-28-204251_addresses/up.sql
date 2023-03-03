CREATE TABLE addresses
(
    account_address       VARCHAR(66) PRIMARY KEY,
    first_appearance_tx   VARCHAR(255) NOT NULL,
    first_appearance_time TIMESTAMP
);
CREATE INDEX addresses_account_address ON addresses (account_address);

