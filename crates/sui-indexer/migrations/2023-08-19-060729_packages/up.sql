CREATE TABLE packages 
(
    package_id                   bytea          PRIMARY KEY,
    -- bcs serialized MovePackage
    move_package                 bytea          NOT NULL
);
