-- SuiAddress and ObjectId type, 0x + 64 chars hex string
CREATE DOMAIN address VARCHAR(66);
-- Max char length for base58 encoded digest
CREATE DOMAIN base58digest VARCHAR(44);

CREATE TABLE addresses
(
    account_address       address PRIMARY KEY,
    first_appearance_tx   base58digest NOT NULL,
    first_appearance_time TIMESTAMP
);
CREATE INDEX addresses_account_address ON addresses (account_address);

