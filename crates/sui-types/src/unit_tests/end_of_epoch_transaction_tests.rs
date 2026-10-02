// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::committee::ProtocolVersion;
use crate::transaction::EndOfEpochTransactionKind;
use sui_protocol_config::{Chain, ProtocolConfig};

#[test]
fn package_config_create_requires_a_package_policy() {
    let mut config = ProtocolConfig::get_for_version(ProtocolVersion::new(1), Chain::Unknown);
    let transaction = EndOfEpochTransactionKind::new_package_config_create();

    assert!(transaction.validity_check(&config).is_err());

    config.set_feature_flag_for_testing("enable_package_minversion".to_string(), true);
    assert!(transaction.validity_check(&config).is_ok());

    config.set_feature_flag_for_testing("enable_package_minversion".to_string(), false);
    config.set_feature_flag_for_testing("enable_package_version_forbid_list".to_string(), true);
    assert!(transaction.validity_check(&config).is_ok());
}
