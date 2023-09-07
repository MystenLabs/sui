CREATE TABLE packages
(
    package_id VARCHAR(66)     NOT NULL,
    version    BIGINT      NOT NULL,
    author     VARCHAR(66)     NOT NULL,
    -- means the column cannot be null,
    -- the element in the array can still be null
    data       JSON NOT NULL,
    CONSTRAINT packages_pk PRIMARY KEY (package_id, version)
);

CREATE INDEX packages_package_id ON packages (package_id);
