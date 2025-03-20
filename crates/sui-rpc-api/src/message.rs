// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub trait MessageMerge<T> {
    fn merge(&mut self, source: T, mask: &crate::field_mask::FieldMaskTree);
}

pub trait MessageFields {
    const FIELDS: &'static [&'static MessageField];
}

pub struct MessageField {
    pub name: &'static str,
    pub message_fields: Option<&'static [&'static MessageField]>,
}

impl MessageField {
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            message_fields: None,
        }
    }

    pub const fn with_message_fields(
        mut self,
        message_fields: &'static [&'static MessageField],
    ) -> Self {
        self.message_fields = Some(message_fields);
        self
    }
}
