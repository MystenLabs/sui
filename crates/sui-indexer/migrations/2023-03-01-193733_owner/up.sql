CREATE TYPE owner_change_type AS ENUM ('new', 'modified', 'deleted');

CREATE TABLE owner_changes
(
    object_id              VARCHAR(66)       NOT NULL,
    version                BIGINT            NOT NULL,
    epoch                  BIGINT            NOT NULL,
    checkpoint             BIGINT            NOT NULL,
    change_type            owner_change_type NOT NULL,
    owner_type             owner_type        NOT NULL,
    owner                  VARCHAR(66),
    initial_shared_version BIGINT,
    object_digest          VARCHAR           NOT NULL,
    object_type            VARCHAR,
    CONSTRAINT owner_changes_pk PRIMARY KEY (epoch, object_id, version)
) PARTITION BY RANGE (epoch);
CREATE INDEX owner_changes_owner_index ON owner_changes (owner_type, owner);
CREATE TABLE owner_changes_partition_0 PARTITION OF owner_changes FOR VALUES FROM (0) TO (1);

CREATE TABLE owner_index
(
    object_id              VARCHAR(66) NOT NULL UNIQUE,
    version                BIGINT      NOT NULL,
    epoch                  BIGINT      NOT NULL,
    owner_type             owner_type  NOT NULL,
    owner                  VARCHAR(66),
    initial_shared_version BIGINT,
    object_digest          VARCHAR     NOT NULL,
    object_type            VARCHAR,
    CONSTRAINT owner_index_pk PRIMARY KEY (object_id)
);
CREATE INDEX owner_index_epoch_index ON owner_index (epoch);
CREATE INDEX owner_index_owner_index ON owner_index (owner_type, owner);



