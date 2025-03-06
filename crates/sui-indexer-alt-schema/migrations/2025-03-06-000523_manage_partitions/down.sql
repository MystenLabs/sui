-- Remove the scheduled job
DELETE FROM cron.job WHERE command = 'SELECT run_maintenance()';

-- Undo partitioning for all tables
-- Group 1
SELECT undo_partition('public.tx_affected_objects', p_keep_table := true);
SELECT undo_partition('public.ev_emit_mod', p_keep_table := true);
SELECT undo_partition('public.ev_struct_inst', p_keep_table := true);
SELECT undo_partition('public.tx_affected_addresses', p_keep_table := true);

-- Group 2
SELECT undo_partition('public.tx_balance_changes', p_keep_table := true);
SELECT undo_partition('public.tx_digests', p_keep_table := true);
SELECT undo_partition('public.tx_kinds', p_keep_table := true);
SELECT undo_partition('public.tx_calls', p_keep_table := true);

-- Group 3
SELECT undo_partition('public.obj_info', p_keep_table := true);
SELECT undo_partition('public.obj_version', p_keep_table := true);
SELECT undo_partition('public.coin_balance_buckets', p_keep_table := true);
