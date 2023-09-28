CREATE TABLE packages 
(
    package_id                   VARCHAR(255)          PRIMARY KEY,
    -- bcs serialized MovePackage
    move_package                 BLOB          NOT NULL
);
