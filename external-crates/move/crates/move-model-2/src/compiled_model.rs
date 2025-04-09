// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::model::{self, WithoutSource};

pub type Model = model::Model<WithoutSource>;
pub type Package<'a> = model::Package<'a, WithoutSource>;
pub type Module<'a> = model::Module<'a, WithoutSource>;
pub type Member<'a> = model::Member<'a, WithoutSource>;
pub type Datatype<'a> = model::Datatype<'a, WithoutSource>;
pub type Struct<'a> = model::Struct<'a, WithoutSource>;
pub type Enum<'a> = model::Enum<'a, WithoutSource>;
pub type Variant<'a> = model::Variant<'a, WithoutSource>;
pub type Function<'a> = model::Function<'a, WithoutSource>;
