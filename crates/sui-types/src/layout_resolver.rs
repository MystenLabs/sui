// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::value::MoveStructLayout;
use crate::{
    object::{Object, ObjectFormatOptions},
    error::SuiError
};


pub trait LayoutResolver {
    fn get_layout(
        &self,
        object: Object,
        format: ObjectFormatOptions,
    ) -> Result<MoveStructLayout, SuiError>;
}
