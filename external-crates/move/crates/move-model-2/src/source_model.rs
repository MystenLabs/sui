// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::model::{self, WithSource};

pub type Model = model::Model<WithSource>;
pub type Package<'a> = model::Package<'a, WithSource>;
pub type Module<'a> = model::Module<'a, WithSource>;
pub type Member<'a> = model::Member<'a, WithSource>;
pub type Datatype<'a> = model::Datatype<'a, WithSource>;
pub type Struct<'a> = model::Struct<'a, WithSource>;
pub type Enum<'a> = model::Enum<'a, WithSource>;
pub type Variant<'a> = model::Variant<'a, WithSource>;
pub type Function<'a> = model::Function<'a, WithSource>;
pub type NamedConstant<'a> = model::NamedConstant<'a>;
