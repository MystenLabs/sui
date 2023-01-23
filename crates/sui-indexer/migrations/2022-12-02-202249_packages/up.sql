CREATE TABLE packages (
    package_id TEXT PRIMARY KEY,
    author TEXT NOT NULL,
    -- means the column cannot be null,
    -- the element in the array can still be null
    module_names TEXT[] NOT NULL,
    package_content TEXT NOT NULL
);

CREATE TABLE package_logs (
    last_processed_id BIGINT PRIMARY KEY
);

INSERT INTO package_logs (last_processed_id) VALUES (0);