-- This file should undo anything in `up.sql`
DROP TABLE IF EXISTS tx_recipients;
DROP INDEX IF EXISTS tx_recipients_tx_sequence_number_index;