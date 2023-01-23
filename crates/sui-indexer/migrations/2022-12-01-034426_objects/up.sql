CREATE TABLE objects (
    object_id VARCHAR(255) PRIMARY KEY,
    version BIGINT NOT NULL,

    -- owner related
    owner_type VARCHAR(255) NOT NULL,
    -- only non-null for objects with an owner,
    -- the owner can be an account or an object. 
    owner_address VARCHAR(255),
    -- only non-null for shared objects
    initial_shared_version BIGINT,

    package_id TEXT NOT NULL,
    transaction_module TEXT NOT NULL,
    object_type TEXT,

    -- status can be CREATED, MUTATED or DELETED.
    object_status VARCHAR(255) NOT NULL
);

CREATE INDEX objects_owner_address ON objects (owner_address);
CREATE INDEX objects_package_id ON objects (package_id);

CREATE TABLE object_logs (
    last_processed_id BIGINT PRIMARY KEY
);

INSERT INTO object_logs (last_processed_id) VALUES (0);
