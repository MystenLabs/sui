CREATE TABLE packages
(
    package_id address     NOT NULL,
    version    BIGINT      NOT NULL,
    author     address     NOT NULL,
    -- means the column cannot be null,
    -- the element in the array can still be null
    data       bcs_bytes[] NOT NULL,
    CONSTRAINT packages_pk PRIMARY KEY (package_id, version)
);

CREATE INDEX packages_package_id ON packages (package_id);
