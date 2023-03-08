CREATE TYPE owner_type AS ENUM ('address_owner', 'object_owner', 'shared', 'immutable');
CREATE TYPE object_status AS ENUM ('created', 'mutated', 'deleted', 'wrapped', 'unwrapped', 'unwrapped_then_deleted');
CREATE TYPE bcs_bytes AS
(
    name TEXT,
    data bytea
);

CREATE TABLE objects
(
    epoch                  BIGINT        NOT NULL,
    checkpoint             BIGINT        NOT NULL,
    object_id              address PRIMARY KEY,
    version                BIGINT        NOT NULL,
    object_digest          base58digest  NOT NULL,
    -- owner related
    owner_type             owner_type    NOT NULL,
    -- only non-null for objects with an owner,
    -- the owner can be an account or an object. 
    owner_address          address,
    -- only non-null for shared objects
    initial_shared_version BIGINT,
    previous_transaction   base58digest  NOT NULL,
    object_type            VARCHAR       NOT NULL,
    object_status          object_status NOT NULL,
    bcs                    bcs_bytes[]   NOT NULL
);
CREATE INDEX objects_owner_address ON objects (owner_type, owner_address);
CREATE INDEX objects_tx_digest ON objects (previous_transaction);

CREATE TABLE objects_history
(
    epoch                  BIGINT        NOT NULL,
    checkpoint             BIGINT        NOT NULL,
    object_id              address       NOT NULL,
    version                BIGINT        NOT NULL,
    object_digest          base58digest  NOT NULL,
    owner_type             owner_type    NOT NULL,
    owner_address          address,
    initial_shared_version BIGINT,
    previous_transaction   base58digest  NOT NULL,
    object_type            VARCHAR       NOT NULL,
    object_status          object_status NOT NULL,
    bcs                    bcs_bytes[]   NOT NULL,
    CONSTRAINT objects_history_pk PRIMARY KEY (epoch, object_id, version)
) PARTITION BY RANGE (epoch);
-- TODO: Add trigger to automatically create partitions at new epoch when we have Epoch Table
CREATE TABLE objects_history_partition_0 PARTITION OF objects_history FOR VALUES FROM (0) TO (1);
CREATE TABLE objects_history_partition_1 PARTITION OF objects_history FOR VALUES FROM (1) TO (2);
CREATE TABLE objects_history_partition_2 PARTITION OF objects_history FOR VALUES FROM (2) TO (3);
CREATE TABLE objects_history_partition_3 PARTITION OF objects_history FOR VALUES FROM (3) TO (4);
CREATE TABLE objects_history_partition_4 PARTITION OF objects_history FOR VALUES FROM (4) TO (5);
CREATE TABLE objects_history_partition_5 PARTITION OF objects_history FOR VALUES FROM (5) TO (6);

CREATE OR REPLACE FUNCTION objects_modified_func() RETURNS TRIGGER AS
$body$
BEGIN
    IF (TG_OP = 'UPDATE' OR TG_OP = 'INSERT') THEN
        INSERT INTO objects_history
        VALUES (NEW.epoch, NEW.checkpoint, NEW.object_id, NEW.version, NEW.object_digest, NEW.owner_type,
                NEW.owner_address,
                NEW.initial_shared_version,
                NEW.previous_transaction, NEW.object_type, NEW.object_status, NEW.bcs);
        RETURN NEW;
    ELSIF (TG_OP = 'DELETE') THEN
        -- object deleted from the main table, archive the history for that object
        DELETE FROM objects_history WHERE object_id = old.object_id;
        RETURN OLD;
    ELSE
        RAISE WARNING '[OBJECTS_MODIFIED_FUNC] - Other action occurred: %, at %',TG_OP,NOW();
        RETURN NULL;
    END IF;

EXCEPTION
    WHEN data_exception THEN
        RAISE WARNING '[OBJECTS_MODIFIED_FUNC] - UDF ERROR [DATA EXCEPTION] - SQLSTATE: %, SQLERRM: %',SQLSTATE,SQLERRM;
        RETURN NULL;
    WHEN unique_violation THEN
        RAISE WARNING '[OBJECTS_MODIFIED_FUNC] - UDF ERROR [UNIQUE] - SQLSTATE: %, SQLERRM: %',SQLSTATE,SQLERRM;
        RETURN NULL;
    WHEN OTHERS THEN
        RAISE WARNING '[OBJECTS_MODIFIED_FUNC] - UDF ERROR [OTHER] - SQLSTATE: %, SQLERRM: %',SQLSTATE,SQLERRM;
        RETURN NULL;
END;
$body$
    LANGUAGE plpgsql;

CREATE TRIGGER objects_history
    AFTER INSERT OR UPDATE OR DELETE
    ON objects
    FOR EACH ROW
EXECUTE PROCEDURE objects_modified_func();

