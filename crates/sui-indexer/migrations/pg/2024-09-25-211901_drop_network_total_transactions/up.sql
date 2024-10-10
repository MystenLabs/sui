ALTER TABLE checkpoints DROP COLUMN network_total_transactions;
UPDATE checkpoints SET max_tx_sequence_number = max_tx_sequence_number + 1 WHERE max_tx_sequence_number IS NOT NULL;
