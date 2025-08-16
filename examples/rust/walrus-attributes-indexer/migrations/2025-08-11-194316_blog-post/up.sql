-- This table maps a blob to its associated Sui Blob object and the latest dynamic field metadata for traceability. The `view_count` is indexed to serve reads on the app.
CREATE TABLE IF NOT EXISTS blog_post (
    -- ID of the Metadata dynamic field
    dynamic_field_id            BYTEA         NOT NULL,
    -- Current version of the Metadata dynamic field
    df_version                  BIGINT        NOT NULL,
    -- Address that published the Walrus Blob
    publisher                   BYTEA         NOT NULL,
    -- The blob ID deterministically derived from the content of a blob and the Walrus configuration
    blob_id                     TEXT          NOT NULL,
    -- Metadata content
    view_count                  BIGINT        NOT NULL,
    title                       TEXT          NOT NULL,
    PRIMARY KEY (dynamic_field_id)
);

-- Index to support ordering and filtering by title
CREATE INDEX IF NOT EXISTS blog_post_by_title ON blog_post
(publisher, title);
