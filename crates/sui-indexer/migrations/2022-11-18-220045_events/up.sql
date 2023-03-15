-- Only user generated events are stored in this table;
-- all other events, including system events and coin balance changes,
-- are handled elsewhere.
CREATE TABLE events
(
    id                 BIGSERIAL PRIMARY KEY,
    transaction_digest base58digest NOT NULL,
    event_sequence     BIGINT       NOT NULL,
    sender             address      NOT NULL,
    package            address      NOT NULL,
    module             TEXT         NOT NULL,
    -- type_ in SuiEvent::MoveEvent
    event_type         TEXT         NOT NULL,
    event_time_ms      BIGINT,
    parsed_json        jsonb        NOT NULL,
    event_bcs          BYTEA        NOT NULL
);

CREATE INDEX events_transaction_digest ON events (transaction_digest);
CREATE INDEX events_sender ON events (sender);
CREATE INDEX events_package ON events (package);
CREATE INDEX events_module ON events (module);
CREATE INDEX events_event_type ON events (event_type);
CREATE INDEX events_event_time_ms ON events (event_time_ms);
