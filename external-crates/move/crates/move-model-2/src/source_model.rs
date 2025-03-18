// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::model::{self, WITH_SOURCE};

pub type Model = model::Model<WITH_SOURCE>;
pub type Package<'a> = model::Package<'a, WITH_SOURCE>;
pub type Module<'a> = model::Module<'a, WITH_SOURCE>;
pub type Member<'a> = model::Member<'a, WITH_SOURCE>;
pub type Datatype<'a> = model::Datatype<'a, WITH_SOURCE>;
pub type Struct<'a> = model::Struct<'a, WITH_SOURCE>;
pub type Enum<'a> = model::Enum<'a, WITH_SOURCE>;
pub type Variant<'a> = model::Variant<'a, WITH_SOURCE>;
pub type Function<'a> = model::Function<'a, WITH_SOURCE>;
pub type NamedConstant<'a> = model::NamedConstant<'a>;
