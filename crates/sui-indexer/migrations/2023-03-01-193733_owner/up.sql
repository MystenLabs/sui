CREATE TABLE owner
(
    epoch         BIGINT        NOT NULL,
    checkpoint    BIGINT        NOT NULL,
    object_id     address PRIMARY KEY,
    version       BIGINT        NOT NULL,
    object_digest base58digest  NOT NULL,
    owner_type    owner_type    NOT NULL,
    owner_address address,
    object_status object_status NOT NULL
);
CREATE INDEX owner_epoch_index ON owner (epoch);
CREATE INDEX owner_owner_index ON owner (owner_type, owner_address);

-- store proc for updating owner table from objects table
CREATE OR REPLACE FUNCTION owner_modified_func() RETURNS TRIGGER AS
$body$
BEGIN
    IF (TG_OP = 'UPDATE') THEN
        -- ignore shared and immutable objects
        IF NEW.owner_type IN ('address_owner', 'object_owner') THEN
            UPDATE owner
            SET epoch         = NEW.epoch,
                checkpoint    = NEW.checkpoint,
                version       = NEW.version,
                object_digest = NEW.object_digest,
                owner_type    = NEW.owner_type,
                owner_address = NEW.owner_address,
                object_status = NEW.object_status
            WHERE object_id = NEW.object_id;
        END IF;
        RETURN NEW;
    ELSIF (TG_OP = 'DELETE') THEN
        DELETE
        FROM owner
        WHERE owner_type = OLD.owner_type
          AND owner_address = OLD.owner_address
          AND object_id = OLD.object_id;
        RETURN OLD;
    ELSIF (TG_OP = 'INSERT') THEN
        IF NEW.owner_type IN ('address_owner', 'object_owner') THEN
            INSERT INTO owner
            VALUES (NEW.epoch, NEW.checkpoint, NEW.object_id, NEW.version, NEW.object_digest, NEW.owner_type,
                    NEW.owner_address, NEW.object_status);
        END IF;
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
    epoch             BIGINT        NOT NULL,
    checkpoint        BIGINT        NOT NULL,
    object_id         address       NOT NULL,
    version           BIGINT        NOT NULL,
    object_digest     base58digest  NOT NULL,
    owner_type        owner_type,
    owner_address     address,
    old_owner_type    owner_type,
    old_owner_address address,
    object_status     object_status NOT NULL,
    CONSTRAINT owner_history_pk PRIMARY KEY (epoch, object_id, version)
) PARTITION BY RANGE (epoch);
CREATE INDEX owner_history_new_owner_index ON owner_history (owner_type, owner_address);
CREATE INDEX owner_history_old_owner_index ON owner_history (old_owner_type, old_owner_address);

CREATE TABLE owner_history_partition_0 PARTITION OF owner_history FOR VALUES FROM (0) TO (1);
-- store proc for updating owner_history table from owner table
CREATE OR REPLACE FUNCTION owner_history_func() RETURNS TRIGGER AS
$body$
BEGIN
    IF (TG_OP = 'UPDATE') THEN
        INSERT INTO owner_history
        VALUES (NEW.epoch, NEW.checkpoint, NEW.object_id, NEW.version, NEW.object_digest, NEW.owner_type,
                NEW.owner_address, OLD.owner_type,
                OLD.owner_address, NEW.object_status);
        RETURN NEW;
    ELSIF (TG_OP = 'DELETE') THEN
        -- owner info deleted from the main table, archive the owner history
        DELETE FROM owner_history WHERE object_id = OLD.object_id;
        RETURN OLD;
    ELSIF (TG_OP = 'INSERT') THEN
        INSERT INTO owner_history
        VALUES (NEW.epoch, NEW.checkpoint, NEW.object_id, NEW.version, NEW.object_digest, NEW.owner_type,
                NEW.owner_address, NULL, NULL, NEW.object_status);
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

CREATE OR REPLACE FUNCTION object_owned_at_checkpoint(_checkpoint BIGINT, _owner_type owner_type, _owner address)
    RETURNS TABLE
            (
                epoch          BIGINT,
                checkpoint     BIGINT,
                object_id      address,
                object_version BIGINT,
                object_digest  base58digest
            )
AS
$func$
BEGIN
    RETURN QUERY
        SELECT diff.epoch, diff.checkpoint, diff.object_id, diff.version, diff.object_digest
        FROM (SELECT o.epoch,
                     o.checkpoint,
                     o.object_id,
                     o.version,
                     o.object_digest,
                     o.owner_address,
                     o.object_status,
                     RANK() OVER (PARTITION BY o.object_id ORDER BY o.version DESC) version_rank
              FROM owner_history AS o
              WHERE ((o.owner_type = _owner_type AND o.owner_address = _owner) OR
                     (o.old_owner_type = _owner_type AND o.old_owner_address = _owner))
                AND o.checkpoint <= _checkpoint) AS diff
        WHERE version_rank = 1
          AND diff.owner_address = _owner
          AND diff.object_status NOT IN ('deleted', 'wrapped', 'unwrapped_then_deleted');
END
$func$
    LANGUAGE plpgsql;
