// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub use checked::*;

#[sui_macros::with_checked_arithmetic]
mod checked {
    use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
    use crate::authority::AuthorityStore;
    use sui_types::executable_transaction::VerifiedExecutableTransaction;
    use sui_types::transaction::{InputObjects, TransactionDataAPI, VersionedProtocolMessage};
    use sui_types::{error::SuiResult, gas::SuiGasStatus};

    pub use sui_transaction_checks::*;

    pub fn check_certificate_input(
        store: &AuthorityStore,
        epoch_store: &AuthorityPerEpochStore,
        cert: &VerifiedExecutableTransaction,
    ) -> SuiResult<(SuiGasStatus, InputObjects)> {
        let protocol_version = epoch_store.protocol_version();

        // This should not happen - validators should not have signed the txn in the first place.
        assert!(
            cert.data()
                .transaction_data()
                .check_version_supported(epoch_store.protocol_config())
                .is_ok(),
            "Certificate formed with unsupported message version {:?} for protocol version {:?}",
            cert.message_version(),
            protocol_version
        );

        let tx_data = &cert.data().intent_message().value;
        let input_object_kinds = tx_data.input_objects()?;
        let input_object_data = if tx_data.is_end_of_epoch_tx() {
            // When changing the epoch, we update a the system object, which is shared, without going
            // through sequencing, so we must bypass the sequence checks here.
            store.check_input_objects(&input_object_kinds, epoch_store.protocol_config())?
        } else {
            store.check_sequenced_input_objects(cert.digest(), &input_object_kinds, epoch_store)?
        };
        let gas_status = get_gas_status(
            &input_object_data,
            tx_data.gas(),
            epoch_store.protocol_config(),
            epoch_store.reference_gas_price(),
            tx_data,
            Some(*cert.digest()),
        )?;
        let input_objects = check_objects(tx_data, input_object_kinds, input_object_data)?;
        // NB: We do not check receiving objects when executing. Only at signing time do we check.
        Ok((gas_status, input_objects))
    }
}
