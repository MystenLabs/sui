// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::schema_v2::packages;
use crate::schema_v2::sql_types::BcsBytes;
use crate::types_v2::IndexedPackage;

use diesel::deserialize::FromSql;
use diesel::pg::{Pg, PgValue};
use diesel::prelude::*;
use diesel::serialize::{Output, ToSql, WriteTuple};
use diesel::sql_types::{Bytea, Nullable, Record, VarChar};
use diesel::SqlType;

#[derive(SqlType, Debug, Clone)]
#[diesel(sql_type = crate::schema_v2::sql_types::BcsBytes)]
pub struct NamedBcsBytes(pub String, pub Vec<u8>);

impl ToSql<Nullable<BcsBytes>, Pg> for NamedBcsBytes {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Pg>) -> diesel::serialize::Result {
        WriteTuple::<(VarChar, Bytea)>::write_tuple(&(self.0.clone(), self.1.clone()), out)
    }
}

impl FromSql<Nullable<BcsBytes>, Pg> for NamedBcsBytes {
    fn from_sql(bytes: PgValue) -> diesel::deserialize::Result<Self> {
        let (name, data) = FromSql::<Record<(VarChar, Bytea)>, Pg>::from_sql(bytes)?;
        Ok(NamedBcsBytes(name, data))
    }
}

#[derive(Queryable, Insertable, Clone, Debug, Identifiable)]
#[diesel(table_name = packages, primary_key(package_id))]
pub struct StoredPackage {
    pub package_id: Vec<u8>,
    pub modules: Vec<NamedBcsBytes>,
}

impl From<IndexedPackage> for StoredPackage {
    fn from(p: IndexedPackage) -> Self {
        Self {
            package_id: p.package_id.to_vec(),
            modules: p
                .move_package
                .serialized_module_map()
                .clone()
                .into_iter()
                .map(|(k, v)| NamedBcsBytes(k, v))
                .collect(),
        }
    }
}
