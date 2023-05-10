// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    error::SuiError,
    object::{MoveObject, ObjectFormatOptions},
};
use move_core_types::value::MoveStructLayout;

pub trait LayoutResolver {
    fn get_layout(
        &mut self,
        object: &MoveObject,
        format: ObjectFormatOptions,
    ) -> Result<MoveStructLayout, SuiError>;
}
