CREATE TABLE packages
(
    package_id                   blob          NOT NULL,
    -- bcs serialized MovePackage
    move_package                 MEDIUMBLOB          NOT NULL,
        CONSTRAINT packages_pk PRIMARY KEY (package_id(255))
);
