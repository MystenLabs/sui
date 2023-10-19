// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;

use crate::context_data::db_data_provider::PgManager;

use super::{
    base64::Base64, move_object::MoveObject, move_value::MoveValue, sui_address::SuiAddress,
};

#[derive(SimpleObject)]
#[graphql(complex)]
pub(crate) struct DynamicField {
    pub name: Option<MoveValue>,
    #[graphql(skip)]
    pub id: SuiAddress,
    #[graphql(skip)]
    pub is_dof: bool,
}

#[ComplexObject]
impl DynamicField {
    async fn id(&self) -> ID {
        self.id.to_string().into()
    }

    async fn value(&self, ctx: &Context<'_>) -> Result<Option<DynamicFieldValue>> {
        let obj = ctx
            .data_unchecked::<PgManager>()
            .fetch_move_obj(self.id, None)
            .await
            .extend()?;

        if self.is_dof {
            Ok(obj.map(DynamicFieldValue::MoveObject))
        } else if let Some(obj) = obj {
            let move_value = obj.contents(ctx).await.extend()?;
            Ok(move_value.map(DynamicFieldValue::MoveValue))
        } else {
            Ok(None)
        }
    }
}

#[derive(Union)]
pub(crate) enum DynamicFieldValue {
    MoveObject(MoveObject), // DynamicObject
    MoveValue(MoveValue),   // DynamicField
}

#[derive(InputObject)] // used as input object
pub(crate) struct DynamicFieldName {
    pub type_: String,
    pub bcs: Base64,
}

#[derive(InputObject)]
pub(crate) struct DynamicFieldFilter {
    pub name_package: Option<SuiAddress>,
    pub name_module: Option<String>,
    pub name_type: Option<String>, // valuePackage -> df_object_type
                                   // valueModule -> df_object_type
                                   // valueType -> df_object_type
}
