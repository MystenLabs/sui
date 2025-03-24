// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::model::{self, WITHOUT_SOURCE};

pub type Model = model::Model<WITHOUT_SOURCE>;
pub type Package<'a> = model::Package<'a, WITHOUT_SOURCE>;
pub type Module<'a> = model::Module<'a, WITHOUT_SOURCE>;
pub type Member<'a> = model::Member<'a, WITHOUT_SOURCE>;
pub type Datatype<'a> = model::Datatype<'a, WITHOUT_SOURCE>;
pub type Struct<'a> = model::Struct<'a, WITHOUT_SOURCE>;
pub type Enum<'a> = model::Enum<'a, WITHOUT_SOURCE>;
pub type Variant<'a> = model::Variant<'a, WITHOUT_SOURCE>;
pub type Function<'a> = model::Function<'a, WITHOUT_SOURCE>;
