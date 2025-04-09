ALTER TABLE token_transfer ADD COLUMN is_finalized BOOLEAN DEFAULT false;
ALTER TABLE token_transfer_data ADD COLUMN is_finalized BOOLEAN DEFAULT false;