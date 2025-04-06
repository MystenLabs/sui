// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::Bcs;
use super::Event;
use super::TransactionEvents;
use crate::message::MessageMergeFrom;
use crate::message::{MessageField, MessageFields, MessageMerge};
use crate::proto::TryFromProtoError;

//
// Event
//

impl Event {
    const PACKAGE_ID_FIELD: &'static MessageField = &MessageField::new("package_id");
    const MODULE_FIELD: &'static MessageField = &MessageField::new("module");
    const SENDER_FIELD: &'static MessageField = &MessageField::new("sender");
    const EVENT_TYPE_FIELD: &'static MessageField = &MessageField::new("event_type");
    const CONTENTS_FIELD: &'static MessageField = &MessageField::new("contents");
}

impl MessageFields for Event {
    const FIELDS: &'static [&'static MessageField] = &[
        Self::PACKAGE_ID_FIELD,
        Self::MODULE_FIELD,
        Self::SENDER_FIELD,
        Self::EVENT_TYPE_FIELD,
        Self::CONTENTS_FIELD,
    ];
}

impl From<sui_sdk_types::Event> for Event {
    fn from(value: sui_sdk_types::Event) -> Self {
        let mut message = Self::default();
        message.merge(value, &crate::field_mask::FieldMaskTree::new_wildcard());
        message
    }
}

impl MessageMerge<sui_sdk_types::Event> for Event {
    fn merge(&mut self, source: sui_sdk_types::Event, mask: &crate::field_mask::FieldMaskTree) {
        if mask.contains(Self::PACKAGE_ID_FIELD.name) {
            self.package_id = Some(source.package_id.to_string());
        }

        if mask.contains(Self::MODULE_FIELD.name) {
            self.module = Some(source.module.to_string());
        }

        if mask.contains(Self::SENDER_FIELD.name) {
            self.sender = Some(source.sender.to_string());
        }

        if mask.contains(Self::EVENT_TYPE_FIELD.name) {
            self.event_type = Some(source.type_.to_string());
        }

        if mask.contains(Self::CONTENTS_FIELD.name) {
            self.contents = Some(Bcs {
                name: Some(source.type_.to_string()),
                value: Some(source.contents.into()),
            });
        }
    }
}

impl MessageMerge<&Event> for Event {
    fn merge(&mut self, source: &Event, mask: &crate::field_mask::FieldMaskTree) {
        let Event {
            package_id,
            module,
            sender,
            event_type,
            contents,
        } = source;

        if mask.contains(Self::PACKAGE_ID_FIELD.name) {
            self.package_id = package_id.clone();
        }

        if mask.contains(Self::MODULE_FIELD.name) {
            self.module = module.clone();
        }

        if mask.contains(Self::SENDER_FIELD.name) {
            self.sender = sender.clone();
        }

        if mask.contains(Self::EVENT_TYPE_FIELD.name) {
            self.event_type = event_type.clone();
        }

        if mask.contains(Self::CONTENTS_FIELD.name) {
            self.contents = contents.clone();
        }
    }
}

impl TryFrom<&Event> for sui_sdk_types::Event {
    type Error = TryFromProtoError;

    fn try_from(value: &Event) -> Result<Self, Self::Error> {
        let package_id = value
            .package_id
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("package_id"))?
            .parse()
            .map_err(TryFromProtoError::from_error)?;

        let module = value
            .module
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("module"))?
            .parse()
            .map_err(TryFromProtoError::from_error)?;

        let sender = value
            .sender
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("sender"))?
            .parse()
            .map_err(TryFromProtoError::from_error)?;

        let type_ = value
            .event_type
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("event_type"))?
            .parse()
            .map_err(TryFromProtoError::from_error)?;

        let contents = value
            .contents
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("contents"))?
            .value()
            .to_vec();

        Ok(Self {
            package_id,
            module,
            sender,
            type_,
            contents,
        })
    }
}

//
// TransactionEvents
//

impl TransactionEvents {
    const BCS_FIELD: &'static MessageField =
        &MessageField::new("bcs").with_message_fields(super::Bcs::FIELDS);
    const DIGEST_FIELD: &'static MessageField = &MessageField::new("digest");
    const EVENTS_FIELD: &'static MessageField =
        &MessageField::new("events").with_message_fields(Event::FIELDS);
}

impl MessageFields for TransactionEvents {
    const FIELDS: &'static [&'static MessageField] =
        &[Self::BCS_FIELD, Self::DIGEST_FIELD, Self::EVENTS_FIELD];
}

impl From<sui_sdk_types::TransactionEvents> for TransactionEvents {
    fn from(value: sui_sdk_types::TransactionEvents) -> Self {
        Self {
            bcs: None,
            digest: Some(value.digest().to_string()),
            events: value.0.into_iter().map(Into::into).collect(),
        }
    }
}

impl MessageMerge<sui_sdk_types::TransactionEvents> for TransactionEvents {
    fn merge(
        &mut self,
        source: sui_sdk_types::TransactionEvents,
        mask: &crate::field_mask::FieldMaskTree,
    ) {
        if mask.contains(Self::BCS_FIELD.name) {
            self.bcs = Some(super::Bcs::serialize(&source).unwrap());
        }

        if mask.contains(Self::DIGEST_FIELD.name) {
            self.digest = Some(source.digest().to_string());
        }

        if let Some(events_mask) = mask.subtree(Self::EVENTS_FIELD.name) {
            self.events = source
                .0
                .into_iter()
                .map(|event| Event::merge_from(event, &events_mask))
                .collect();
        }
    }
}

impl MessageMerge<&TransactionEvents> for TransactionEvents {
    fn merge(&mut self, source: &TransactionEvents, mask: &crate::field_mask::FieldMaskTree) {
        let TransactionEvents {
            bcs,
            digest,
            events,
        } = source;

        if mask.contains(Self::BCS_FIELD.name) {
            self.bcs = bcs.clone();
        }

        if mask.contains(Self::DIGEST_FIELD.name) {
            self.digest = digest.clone();
        }

        if let Some(events_mask) = mask.subtree(Self::EVENTS_FIELD.name) {
            self.events = events
                .iter()
                .map(|event| Event::merge_from(event, &events_mask))
                .collect();
        }
    }
}

impl TryFrom<&TransactionEvents> for sui_sdk_types::TransactionEvents {
    type Error = TryFromProtoError;

    fn try_from(value: &TransactionEvents) -> Result<Self, Self::Error> {
        Ok(Self(
            value
                .events
                .iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
        ))
    }
}
