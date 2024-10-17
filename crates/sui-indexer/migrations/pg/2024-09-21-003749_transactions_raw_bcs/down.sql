-- This file should undo anything in `up.sql`
ALTER TABLE transactions DROP COLUMN raw_transactions_sigs;
