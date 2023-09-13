DO
$$
    BEGIN
        CREATE TYPE bcs_bytes AS
        (
            name TEXT,
            data bytea
        );
    EXCEPTION
        WHEN duplicate_object THEN
            -- Type already exists, do nothing
            NULL;
    END
$$;

CREATE TABLE packages 
(
    package_id                   bytea          PRIMARY KEY,
    modules                      bcs_bytes[]    NOT NULL
);
