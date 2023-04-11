#!/bin/bash
set -e

# Update pg_hba.conf
echo "local all all trust" >> "$PGDATA/pg_hba.conf"
echo "host all all 127.0.0.1/32 md5" >> "$PGDATA/pg_hba.conf"
echo "host all all ::1/128 md5" >> "$PGDATA/pg_hba.conf"

# Update postgresql.conf
echo "log_connections = on" >> "$PGDATA/postgresql.conf"
echo "log_disconnections = on" >> "$PGDATA/postgresql.conf"
echo "log_hostname = on" >> "$PGDATA/postgresql.conf"

# Start PostgreSQL
exec docker-entrypoint.sh postgres
