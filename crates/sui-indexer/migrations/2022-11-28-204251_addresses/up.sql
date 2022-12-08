CREATE TABLE addresses (
    account_address VARCHAR(255) PRIMARY KEY,
    first_appearance_tx VARCHAR(255) NOT NULL, 
    first_appearance_time TIMESTAMP
);

CREATE TABLE address_logs (
    -- this is essentially BIGSERIAL starting from 1
    -- see https://www.postgresql.org/docs/9.1/datatype-numeric.html
    last_processed_id BIGINT PRIMARY KEY
);

-- last processed serial number, as the serial number starts from 1,
-- initial value of last_processed_id should be 0.
INSERT INTO address_logs VALUES (0);

