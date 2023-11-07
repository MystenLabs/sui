-- This file should undo anything in `up.sql`
DROP TABLE IF EXISTS tx_senders;
DROP INDEX IF EXISTS tx_senders_tx_sequence_number_index;