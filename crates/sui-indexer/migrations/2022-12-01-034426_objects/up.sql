CREATE TYPE owner_type AS ENUM ('address_owner', 'object_owner', 'shared', 'immutable');
CREATE TYPE change_type AS ENUM ('new', 'modify', 'delete');

CREATE TABLE objects
(
    epoch                  BIGINT       NOT NULL,
    checkpoint             BIGINT       NOT NULL,
    object_id              address PRIMARY KEY,
    version                BIGINT       NOT NULL,
    object_digest          base58digest NOT NULL,
    -- owner related
    owner_type             owner_type   NOT NULL,
    -- only non-null for objects with an owner,
    -- the owner can be an account or an object. 
    owner_address          address,
    -- only non-null for shared objects
    initial_shared_version BIGINT,
    previous_transaction   base58digest NOT NULL,
    package_id             address      NOT NULL,
    transaction_module     VARCHAR      NOT NULL,
    object_type            VARCHAR      NOT NULL
);
CREATE INDEX objects_owner_address ON objects (owner_address);
CREATE INDEX objects_package_id ON objects (package_id);

CREATE TABLE objects_history
(
    epoch                  BIGINT       NOT NULL,
    checkpoint             BIGINT       NOT NULL,
    object_id              address      NOT NULL,
    version                BIGINT       NOT NULL,
    object_digest          base58digest NOT NULL,
    owner_type             owner_type   NOT NULL,
    owner_address          address,
    initial_shared_version BIGINT,
    previous_transaction   base58digest NOT NULL,
    package_id             address      NOT NULL,
    transaction_module     VARCHAR      NOT NULL,
    object_type            VARCHAR      NOT NULL,
    object_status          change_type  NOT NULL,
    CONSTRAINT objects_history_pk PRIMARY KEY (object_id, version)
);

CREATE OR REPLACE FUNCTION objects_modified_func() RETURNS TRIGGER AS
$body$
BEGIN
    IF (TG_OP = 'UPDATE') THEN
        INSERT INTO objects_history
        VALUES (NEW.epoch, NEW.checkpoint, NEW.object_id, NEW.version, NEW.object_digest, NEW.owner_type,
                NEW.owner_address,
                NEW.initial_shared_version,
                NEW.previous_transaction, NEW.package_id, NEW.transaction_module, NEW.object_type, 'modify');
        RETURN NEW;
    ELSIF (TG_OP = 'DELETE') THEN
        INSERT INTO objects_history
        VALUES (NEW.epoch, NEW.checkpoint, NEW.object_id, NEW.version, NEW.object_digest, NEW.owner_type,
                NEW.owner_address,
                NEW.initial_shared_version,
                NEW.previous_transaction, NEW.package_id, NEW.transaction_module, NEW.object_type, 'delete');
        RETURN OLD;
    ELSIF (TG_OP = 'INSERT') THEN
        INSERT INTO objects_history
        VALUES (NEW.epoch, NEW.checkpoint, NEW.object_id, NEW.version, NEW.object_digest, NEW.owner_type,
                NEW.owner_address,
                NEW.initial_shared_version,
                NEW.previous_transaction, NEW.package_id, NEW.transaction_module, NEW.object_type, 'new');
        RETURN NEW;
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

