CREATE TABLE epochs
(
    epoch                           bigint      PRIMARY KEY,
    first_checkpoint_id             bigint      NOT NULL,
    -- array of bcs serialization of (AuthorityName, StakeUnit) tuples,
    -- can be extracted from Committee type.
    voting_rights                   bytea[]     NOT NULL
);
