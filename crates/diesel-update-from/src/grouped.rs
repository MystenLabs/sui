// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::expression::{Expression, ValidGrouping};
use diesel::pg::Pg;
use diesel::query_builder::*;
use diesel::result::QueryResult;
use diesel::sql_types::DieselNumericOps;

/// A copy of diesel's construct for ...a pair of parentheses (because their copy is private).
#[derive(Debug, Copy, Clone, QueryId, Default, DieselNumericOps, ValidGrouping)]
pub struct Grouped<T>(pub T);

impl<T: Expression> Expression for Grouped<T> {
    type SqlType = T::SqlType;
}

impl<T> QueryFragment<Pg> for Grouped<T>
where
    T: QueryFragment<Pg>,
{
    fn walk_ast<'b>(&'b self, mut out: AstPass<'_, 'b, Pg>) -> QueryResult<()> {
        out.push_sql("(");
        self.0.walk_ast(out.reborrow())?;
        out.push_sql(")");
        Ok(())
    }
}
