// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::Epoch;
use super::ProtocolConfig;
use crate::message::MessageMerge;
use crate::message::MessageMergeFrom;
use crate::message::{MessageField, MessageFields};

impl Epoch {
    pub const EPOCH_FIELD: &'static MessageField = &MessageField::new("epoch");
    pub const COMMITTEE_FIELD: &'static MessageField = &MessageField::new("committee");
    pub const SYSTEM_STATE_FIELD: &'static MessageField = &MessageField::new("system_state");
    pub const FIRST_CHECKPOINT_FIELD: &'static MessageField =
        &MessageField::new("first_checkpoint");
    pub const LAST_CHECKPOINT_FIELD: &'static MessageField = &MessageField::new("last_checkpoint");
    pub const START_FIELD: &'static MessageField = &MessageField::new("start");
    pub const END_FIELD: &'static MessageField = &MessageField::new("end");
    pub const REFERENCE_GAS_PRICE_FIELD: &'static MessageField =
        &MessageField::new("reference_gas_price");
    pub const PROTOCOL_CONFIG_FIELD: &'static MessageField =
        &MessageField::new("protocol_config").with_message_fields(ProtocolConfig::FIELDS);
}

impl MessageFields for Epoch {
    const FIELDS: &'static [&'static MessageField] = &[
        Self::EPOCH_FIELD,
        Self::COMMITTEE_FIELD,
        Self::SYSTEM_STATE_FIELD,
        Self::FIRST_CHECKPOINT_FIELD,
        Self::LAST_CHECKPOINT_FIELD,
        Self::START_FIELD,
        Self::END_FIELD,
        Self::REFERENCE_GAS_PRICE_FIELD,
        Self::PROTOCOL_CONFIG_FIELD,
    ];
}

impl MessageMerge<&Epoch> for Epoch {
    fn merge(&mut self, source: &Epoch, mask: &crate::field_mask::FieldMaskTree) {
        let Epoch {
            epoch,
            committee,
            system_state,
            first_checkpoint,
            last_checkpoint,
            start,
            end,
            reference_gas_price,
            protocol_config,
        } = source;

        if mask.contains(Self::EPOCH_FIELD.name) {
            self.epoch = *epoch;
        }

        if mask.contains(Self::COMMITTEE_FIELD.name) {
            self.committee = committee.to_owned();
        }

        if mask.contains(Self::SYSTEM_STATE_FIELD.name) {
            self.system_state = system_state.to_owned();
        }

        if mask.contains(Self::FIRST_CHECKPOINT_FIELD.name) {
            self.first_checkpoint = first_checkpoint.to_owned();
        }

        if mask.contains(Self::LAST_CHECKPOINT_FIELD.name) {
            self.last_checkpoint = last_checkpoint.to_owned();
        }

        if mask.contains(Self::START_FIELD.name) {
            self.start = start.to_owned();
        }

        if mask.contains(Self::END_FIELD.name) {
            self.end = end.to_owned();
        }

        if mask.contains(Self::REFERENCE_GAS_PRICE_FIELD.name) {
            self.reference_gas_price = reference_gas_price.to_owned();
        }

        if let Some(submask) = mask.subtree(Self::PROTOCOL_CONFIG_FIELD.name) {
            self.protocol_config = protocol_config
                .as_ref()
                .map(|config| ProtocolConfig::merge_from(config, &submask));
        }
    }
}

impl super::GetEpochRequest {
    pub const READ_MASK_DEFAULT: &str =
        "epoch,committee,first_checkpoint,last_checkpoint,start,end,reference_gas_price,protocol_config.protocol_version";
}

impl ProtocolConfig {
    pub const PROTOCOL_VERSION_FIELD: &'static MessageField =
        &MessageField::new("protocol_version");
    pub const FEATURE_FLAGS_FIELD: &'static MessageField = &MessageField::new("feature_flags");
    pub const ATTRIBUTES_FIELD: &'static MessageField = &MessageField::new("attributes");
}

impl MessageFields for ProtocolConfig {
    const FIELDS: &'static [&'static MessageField] = &[
        Self::PROTOCOL_VERSION_FIELD,
        Self::FEATURE_FLAGS_FIELD,
        Self::ATTRIBUTES_FIELD,
    ];
}

impl MessageMerge<&ProtocolConfig> for ProtocolConfig {
    fn merge(&mut self, source: &ProtocolConfig, mask: &crate::field_mask::FieldMaskTree) {
        let ProtocolConfig {
            protocol_version,
            feature_flags,
            attributes,
        } = source;

        if mask.contains(Self::PROTOCOL_VERSION_FIELD.name) {
            self.protocol_version = *protocol_version;
        }

        if mask.contains(Self::FEATURE_FLAGS_FIELD.name) {
            self.feature_flags = feature_flags.to_owned();
        }

        if mask.contains(Self::ATTRIBUTES_FIELD.name) {
            self.attributes = attributes.to_owned();
        }
    }
}

impl MessageMerge<ProtocolConfig> for ProtocolConfig {
    fn merge(&mut self, source: ProtocolConfig, mask: &crate::field_mask::FieldMaskTree) {
        let ProtocolConfig {
            protocol_version,
            feature_flags,
            attributes,
        } = source;

        if mask.contains(Self::PROTOCOL_VERSION_FIELD.name) {
            self.protocol_version = protocol_version;
        }

        if mask.contains(Self::FEATURE_FLAGS_FIELD.name) {
            self.feature_flags = feature_flags;
        }

        if mask.contains(Self::ATTRIBUTES_FIELD.name) {
            self.attributes = attributes;
        }
    }
}
