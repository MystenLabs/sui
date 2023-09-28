CREATE TABLE tx_indices (
    tx_sequence_number          BIGINT       PRIMARY KEY,
    checkpoint_sequence_number  BIGINT       NOT NULL,
    -- bytes of the transaction digest
    transaction_digest          VARCHAR(255)        NOT NULL,
    -- array of ObjectID in bytes.
    input_objects               JSON      NOT NULL,
    -- array of ObjectID in bytes
    changed_objects             JSON      NOT NULL,
    -- array of SuiAddress in bytes. All signers of the transaction.
    senders                     JSON      NOT NULL,
    -- array of SuiAddress in bytes. All gas owners of the transaction.
    payers                      JSON      NOT NULL,
    -- array of SuiAddress in bytes. 
    recipients                  JSON      NOT NULL,
    -- array of PackageID in bytes of all MoveCalls of the transaction.
    packages                    JSON      NOT NULL,
    -- array of "package::module" of all MoveCalls of the transaction.
    -- e.g. "0x0000000000000000000000000000000000000000000000000000000000000003::sui_system"
    package_modules             JSON      NOT NULL,
    -- array of "package::module::function" of all MoveCalls of the transaction.
    -- e.g. "0x0000000000000000000000000000000000000000000000000000000000000003::sui_system::request_add_stake"
    package_module_functions    JSON      NOT NULL
);

-- TODO: mysql does not support array or GIN index, need separate tables to join these.
-- CREATE INDEX tx_indices_input_objects ON tx_indices USING GIN(input_objects);
-- CREATE INDEX tx_indices_changed_objects ON tx_indices USING GIN(changed_objects);
-- CREATE INDEX tx_indices_senders ON tx_indices USING GIN(senders);
-- CREATE INDEX tx_indices_recipients ON tx_indices USING GIN(recipients);
-- CREATE INDEX tx_indices_package ON tx_indices USING GIN(packages);
-- CREATE INDEX tx_indices_package_module ON tx_indices USING GIN(package_modules);
-- CREATE INDEX tx_indices_package_module_function ON tx_indices USING GIN(package_module_functions);
CREATE INDEX tx_indices_checkpoint_sequence_number ON tx_indices (checkpoint_sequence_number);
