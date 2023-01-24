CREATE TABLE packages (
    id BIGSERIAL PRIMARY KEY,
    package_id TEXT NOT NULL UNIQUE,
    author TEXT NOT NULL,
    -- means the column cannot be null,
    -- the element in the array can stil be null
    module_names TEXT[] NOT NULL,
    package_content TEXT NOT NULL
);

CREATE INDEX packages_package_id ON packages (package_id);

CREATE TABLE package_logs (
    last_processed_id BIGINT PRIMARY KEY
);

INSERT INTO package_logs (last_processed_id) VALUES (0);