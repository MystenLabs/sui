-- Indexing table mapping an object's ID and version to its checkpoint
-- sequence number, partitioned by the first byte of its Object ID.
CREATE TABLE objects_version (
    object_id           bytea         NOT NULL,
    object_version      bigint        NOT NULL,
    cp_sequence_number  bigint        NOT NULL,
    PRIMARY KEY (object_id, object_version)
) PARTITION BY RANGE (object_id);

-- Create a partition for each first byte value.
DO $$
DECLARE
    lo text;
    hi text;
BEGIN
    FOR i IN 0..254 LOOP
        lo := LPAD(TO_HEX(i), 2, '0');
        hi := LPAD(TO_HEX(i + 1), 2, '0');
        EXECUTE FORMAT($F$
            CREATE TABLE objects_version_%1$s PARTITION OF objects_version FOR VALUES
            FROM (E'\\x%1$s00000000000000000000000000000000000000000000000000000000000000')
            TO   (E'\\x%2$s00000000000000000000000000000000000000000000000000000000000000');
        $F$, lo, hi);
    END LOOP;
END;
$$ LANGUAGE plpgsql;

-- Special case for the last partition, because of the upper bound.
CREATE TABLE objects_version_ff PARTITION OF objects_version FOR VALUES
FROM (E'\\xff00000000000000000000000000000000000000000000000000000000000000')
TO   (MAXVALUE);
