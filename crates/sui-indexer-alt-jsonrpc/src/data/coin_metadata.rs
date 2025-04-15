// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::HashMap, sync::Arc};

use async_graphql::dataloader::Loader;
use diesel::{BoolExpressionMethods, ExpressionMethods, JoinOnDsl, QueryDsl};
use move_core_types::language_storage::StructTag;
use sui_indexer_alt_schema::{objects::StoredObjInfo, schema::obj_info};
use sui_types::{
    coin::{COIN_METADATA_STRUCT_NAME, COIN_MODULE_NAME},
    TypeTag, SUI_FRAMEWORK_ADDRESS,
};

use crate::data::error::Error;

use super::pg_reader::PgReader;

/// Key for fetching the  of a CoinMetadata object, based on its coin marker type, e.g.
/// `0x2::sui::SUI`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct CoinMetadataKey(pub StructTag);

#[async_trait::async_trait]
impl Loader<CoinMetadataKey> for PgReader {
    type Value = StoredObjInfo;
    type Error = Arc<Error>;

    async fn load(
        &self,
        keys: &[CoinMetadataKey],
    ) -> Result<HashMap<CoinMetadataKey, StoredObjInfo>, Self::Error> {
        use obj_info::dsl as o;

        let (candidates, newer) = diesel::alias!(obj_info as candidates, obj_info as newer);

        macro_rules! candidates {
            ($($field:ident),* $(,)?) => {
                candidates.fields(($(o::$field),*))
            };
        }

        macro_rules! newer {
            ($($field:ident),* $(,)?) => {
                newer.fields(($(o::$field),*))
            };
        }

        if keys.is_empty() {
            return Ok(HashMap::new());
        }

        let mut conn = self.connect().await.map_err(Arc::new)?;

        let instantiations = keys
            .iter()
            .map(|CoinMetadataKey(tag)| {
                let params: Vec<TypeTag> = vec![tag.clone().into()];
                bcs::to_bytes(&params)
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| Arc::new(Error::Serde(e.into())))?;

        let query = candidates
            .distinct_on(candidates!(package, module, name, instantiation))
            .left_join(
                newer.on(candidates!(object_id)
                    .eq(newer!(object_id))
                    .and(candidates!(cp_sequence_number).lt(newer!(cp_sequence_number)))),
            )
            .select(candidates!(
                object_id,
                cp_sequence_number,
                owner_kind,
                owner_id,
                package,
                module,
                name,
                instantiation,
            ))
            .filter(newer!(object_id).is_null())
            .filter(candidates!(package).eq(SUI_FRAMEWORK_ADDRESS.into_bytes()))
            .filter(candidates!(module).eq(COIN_MODULE_NAME.as_str()))
            .filter(candidates!(name).eq(COIN_METADATA_STRUCT_NAME.as_str()))
            .filter(candidates!(instantiation).eq_any(&instantiations));

        let obj_info: Vec<StoredObjInfo> = conn.results(query).await.map_err(Arc::new)?;
        let instantiations_to_stored: HashMap<_, _> = obj_info
            .iter()
            .map(|stored| (&stored.instantiation, stored))
            .collect();

        Ok(keys
            .iter()
            .zip(instantiations)
            .filter_map(|(key, inst)| {
                let stored = *instantiations_to_stored.get(&Some(inst))?;
                Some((key.clone(), stored.clone()))
            })
            .collect())
    }
}
