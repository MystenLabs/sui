CREATE TABLE owner
(
    epoch                  BIGINT       NOT NULL,
    checkpoint             BIGINT       NOT NULL,
    object_id              address PRIMARY KEY,
    version                BIGINT       NOT NULL,
    object_digest          base58digest NOT NULL,
    owner_type             owner_type   NOT NULL,
    owner_address          address,
    initial_shared_version BIGINT
);
CREATE INDEX owner_epoch_index ON owner (epoch);
CREATE INDEX owner_owner_index ON owner (owner_type, owner_address);

-- store proc for updating owner table from objects table
CREATE OR REPLACE FUNCTION owner_modified_func() RETURNS TRIGGER AS
$body$
BEGIN
    IF (TG_OP = 'UPDATE') THEN
        UPDATE owner
        SET epoch                  = NEW.epoch,
            checkpoint             = NEW.checkpoint,
            version                = NEW.version,
            object_digest          = NEW.object_digest,
            owner_type             = NEW.owner_type,
            owner_address          = NEW.owner_address,
            initial_shared_version = NEW.initial_shared_version
        WHERE object_id = NEW.object_id;
        RETURN NEW;
    ELSIF (TG_OP = 'DELETE') THEN
        DELETE FROM owner WHERE object_id == OLD.object_id;
        RETURN OLD;
    ELSIF (TG_OP = 'INSERT') THEN
        INSERT INTO owner
        VALUES (NEW.epoch, NEW.checkpoint, NEW.object_id, NEW.version, NEW.object_digest, NEW.owner_type,
                NEW.owner_address,
                NEW.initial_shared_version);
        RETURN NEW;
    ELSE
        RAISE WARNING '[OWNER_MODIFIED_FUNC] - Other action occurred: %, at %',TG_OP,NOW();
        RETURN NULL;
    END IF;

EXCEPTION
    WHEN data_exception THEN
        RAISE WARNING '[OWNER_MODIFIED_FUNC] - UDF ERROR [DATA EXCEPTION] - SQLSTATE: %, SQLERRM: %',SQLSTATE,SQLERRM;
        RETURN NULL;
    WHEN unique_violation THEN
        RAISE WARNING '[OWNER_MODIFIED_FUNC] - UDF ERROR [UNIQUE] - SQLSTATE: %, SQLERRM: %',SQLSTATE,SQLERRM;
        RETURN NULL;
    WHEN OTHERS THEN
        RAISE WARNING '[OWNER_MODIFIED_FUNC] - UDF ERROR [OTHER] - SQLSTATE: %, SQLERRM: %',SQLSTATE,SQLERRM;
        RETURN NULL;
END;
$body$
    LANGUAGE plpgsql;

CREATE TRIGGER owner
    AFTER INSERT OR UPDATE OR DELETE
    ON objects
    FOR EACH ROW
EXECUTE PROCEDURE owner_modified_func();

CREATE TABLE owner_history
(
    epoch                  BIGINT       NOT NULL,
    checkpoint             BIGINT       NOT NULL,
    object_id              address      NOT NULL,
    version                BIGINT       NOT NULL,
    object_digest          base58digest NOT NULL,
    owner_type             owner_type   NOT NULL,
    owner_address          address,
    initial_shared_version BIGINT,
    change_type            change_type  NOT NULL,
    CONSTRAINT owner_history_pk PRIMARY KEY (epoch, object_id, version)
) PARTITION BY RANGE (epoch);
CREATE INDEX owner_history_owner_index ON owner_history (owner_type, owner_address);
-- TODO: Add trigger to automatically create partitions at new epoch when we have Epoch Table
CREATE TABLE owner_history_partition_0 PARTITION OF owner_history FOR VALUES FROM (0) TO (1);
CREATE TABLE owner_history_partition_1 PARTITION OF owner_history FOR VALUES FROM (1) TO (2);
CREATE TABLE owner_history_partition_2 PARTITION OF owner_history FOR VALUES FROM (2) TO (3);
CREATE TABLE owner_history_partition_3 PARTITION OF owner_history FOR VALUES FROM (3) TO (4);
CREATE TABLE owner_history_partition_4 PARTITION OF owner_history FOR VALUES FROM (4) TO (5);
CREATE TABLE owner_history_partition_5 PARTITION OF owner_history FOR VALUES FROM (5) TO (6);

-- store proc for updating owner_history table from owner table
CREATE OR REPLACE FUNCTION owner_history_func() RETURNS TRIGGER AS
$body$
BEGIN
    IF (TG_OP = 'UPDATE') THEN
        IF NEW.owner_address != OLD.owner_address THEN
            INSERT INTO owner_history
            VALUES (NEW.epoch, NEW.checkpoint, NEW.object_id, NEW.version, NEW.object_digest, NEW.owner_type,
                    NEW.owner_address,
                    NEW.initial_shared_version, 'modify');
            RETURN NEW;
        END IF;
    ELSIF (TG_OP = 'DELETE') THEN
        INSERT INTO owner_history
        VALUES (NEW.epoch, NEW.checkpoint, NEW.object_id, NEW.version, NEW.object_digest, NEW.owner_type,
                NEW.owner_address,
                NEW.initial_shared_version, 'delete');
        RETURN OLD;
    ELSIF (TG_OP = 'INSERT') THEN
        INSERT INTO owner_history
        VALUES (NEW.epoch, NEW.checkpoint, NEW.object_id, NEW.version, NEW.object_digest, NEW.owner_type,
                NEW.owner_address,
                NEW.initial_shared_version, 'new');
        RETURN NEW;
    ELSE
        RAISE WARNING '[OWNER_HISTORY_FUNC] - Other action occurred: %, at %',TG_OP,NOW();
        RETURN NULL;
    END IF;

EXCEPTION
    WHEN data_exception THEN
        RAISE WARNING '[OWNER_HISTORY_FUNC] - UDF ERROR [DATA EXCEPTION] - SQLSTATE: %, SQLERRM: %',SQLSTATE,SQLERRM;
        RETURN NULL;
    WHEN unique_violation THEN
        RAISE WARNING '[OWNER_HISTORY_FUNC] - UDF ERROR [UNIQUE] - SQLSTATE: %, SQLERRM: %',SQLSTATE,SQLERRM;
        RETURN NULL;
    WHEN OTHERS THEN
        RAISE WARNING '[OWNER_HISTORY_FUNC] - UDF ERROR [OTHER] - SQLSTATE: %, SQLERRM: %',SQLSTATE,SQLERRM;
        RETURN NULL;
END;
$body$
    LANGUAGE plpgsql;

CREATE TRIGGER owner_history
    AFTER INSERT OR UPDATE OR DELETE
    ON owner
    FOR EACH ROW
EXECUTE PROCEDURE owner_history_func();
