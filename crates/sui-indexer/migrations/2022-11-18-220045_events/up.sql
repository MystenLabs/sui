-- Only user generated events are stored in this table;
-- all other events, including system events and coin balance changes,
-- are handled elsewhere.
CREATE TABLE events
(
    id                 BIGINT AUTO_INCREMENT PRIMARY KEY,
    transaction_digest VARCHAR(44) NOT NULL,
    event_sequence     BIGINT       NOT NULL,
    sender             VARCHAR(66)      NOT NULL,
    package            VARCHAR(66)      NOT NULL,
    module             TEXT        NOT NULL,
    event_type         TEXT        NOT NULL,
    event_time_ms      BIGINT,
    event_bcs          BLOB        NOT NULL
);

CREATE INDEX events_transaction_digest ON events (transaction_digest);
CREATE INDEX events_sender ON events (sender);
CREATE INDEX events_package ON events (package);
CREATE INDEX events_module ON events (module(255));
CREATE INDEX events_event_type ON events (event_type(255));
CREATE INDEX events_event_time_ms ON events (event_time_ms);
