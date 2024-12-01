// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::{
    insertable::InsertValues,
    pg::Pg,
    query_builder::{AstPass, BatchInsert, QueryFragment, QueryId, ValuesClause},
    AsChangeset, Insertable, QueryResult, RunQueryDsl, Table,
};

#[cfg(test)]
mod tests;

/// Diesel bindings for the postgres specific Bulk-`UPDATE` statement, of the form:
///
/// ```sql
/// UPDATE
///     $table
/// SET
///     $set, ...
/// FROM
///     (VALUES $values, ...) AS excluded ($columns, ...)
/// WHERE
///     $where_clause;
/// ```
#[derive(Debug)]
pub struct UpdateFromStatement<T: Table, S = SetNotCalled, V = ValuesNotCalled, W = WhereNotCalled>
{
    table: T::FromClause,
    set_clause: S,
    values: V,
    where_clause: W,
}

pub struct SetNotCalled;
pub struct ValuesNotCalled;
pub struct WhereNotCalled;

impl<T: Table, V, W> UpdateFromStatement<T, SetNotCalled, V, W> {
    /// Provides the `SET` clause of the `UPDATE` statement.
    ///
    /// This clause decides which fields in the table are updated with the values from the `VALUES`
    /// clause.
    pub fn set<S>(self, set_clause: S) -> UpdateFromStatement<T, S::Changeset, V, W>
    where
        S: AsChangeset<Target = T>,
    {
        UpdateFromStatement {
            table: self.table,
            set_clause: set_clause.as_changeset(),
            values: self.values,
            where_clause: self.where_clause,
        }
    }
}

impl<T: Table, S, W> UpdateFromStatement<T, S, ValuesNotCalled, W> {
    /// Provides the `VALUES` clause of the `UPDATE` statement.
    ///
    /// This clause provides potentially multiple values to write into the table. `values` must be
    /// `Insertable` into the `table` being updated, and can contain values that are not actually
    /// written to the table (including, for example, the key columns used to identify which rows
    /// to associate each value with for the update).
    pub fn values<V>(self, values: V) -> UpdateFromStatement<T, S, V::Values, W>
    where
        V: Insertable<T>,
    {
        UpdateFromStatement {
            table: self.table,
            set_clause: self.set_clause,
            values: values.values(),
            where_clause: self.where_clause,
        }
    }
}

impl<T: Table, S, V> UpdateFromStatement<T, S, V, WhereNotCalled> {
    /// Restrict the rows that are updated by the `UPDATE` statement.
    ///
    /// This kind of query must have a filter supplied, to join the values being updated from with
    /// the rows they are updating. Without a filter, all rows in the table will be updated using
    /// values from the first value in the `VALUES` clause.
    pub fn filter<W>(self, where_clause: W) -> UpdateFromStatement<T, S, V, W> {
        UpdateFromStatement {
            table: self.table,
            set_clause: self.set_clause,
            values: self.values,
            where_clause,
        }
    }
}

/// `QueryId` is used to support prepared statement cachine. The `values` clause causes complicates
/// for the `UPDATE ... FROM` statement, because it will generate a different prepared statement
/// based on the number of values being inserted.
impl<T: Table, S, V, W> QueryId for UpdateFromStatement<T, S, V, W> {
    type QueryId = ();
    const HAS_STATIC_QUERY_ID: bool = false;
}

impl<T: Table, S, V, W, Conn> RunQueryDsl<Conn> for UpdateFromStatement<T, S, V, W> {}

impl<T: Table, S, V, W, QId, const HAS_STATIC_QUERY_ID: bool> QueryFragment<Pg>
    for UpdateFromStatement<
        T,
        S,
        BatchInsert<Vec<ValuesClause<V, T>>, T, QId, HAS_STATIC_QUERY_ID>,
        W,
    >
where
    T::FromClause: QueryFragment<Pg>,
    S: QueryFragment<Pg>,
    V: InsertValues<Pg, T>,
    W: QueryFragment<Pg>,
{
    fn walk_ast<'b>(&'b self, mut out: AstPass<'_, 'b, Pg>) -> QueryResult<()> {
        out.unsafe_to_cache_prepared();
        out.push_sql("UPDATE ");
        self.table.walk_ast(out.reborrow())?;
        out.push_sql(" SET ");
        self.set_clause.walk_ast(out.reborrow())?;

        let mut values = self.values.values.iter();
        let Some(first) = values.next() else {
            out.push_sql(" WHERE 1=0");
            return Ok(());
        };

        out.push_sql(" FROM (VALUES (");
        first.values.walk_ast(out.reborrow())?;
        out.push_sql(")");

        for value in values {
            out.push_sql(", (");
            value.values.walk_ast(out.reborrow())?;
            out.push_sql(")");
        }

        out.push_sql(") AS excluded (");
        first.values.column_names(out.reborrow())?;
        out.push_sql(") WHERE ");
        self.where_clause.walk_ast(out.reborrow())?;
        Ok(())
    }
}

/// Creates an `UPDATE` statement where the values to update are provided by a `VALUES` clause
/// (i.e. a bulk update). A Postgres-specific extension, which Diesel does not support natively.
///
/// The `values` being updated must be `Insertable` into the `table` being updated.
///
/// The fields that are updated are determined by the `set` clause, and the `filter` clause must be
/// used to join the values being updated from with the rows in the table they are updating. The
/// set of values being updated from can be referred to as the `excluded` table (similar to how the
/// regular [diesel::update] works) in both the `set` and `filter` clauses.
///
/// NOTE: By default, diesel treats `None` fields being inserted as equivalent to `DEFAULT`, (for
/// `INSERT` statements), but this syntax is not compatible with the `UPDATE ... FROM` statement,
/// where `None` fields must be treated as `NULL`. To work around this, for types that are only
/// used for bulk updates, you can set:
///
///  #[diesel(treat_none_as_default_value = false)]
///
/// Which will cause `None` to be interpreted as `NULL`, or, if `DEFAULT` is useful in some cases,
/// use `Option<Option<T>>`: `None` will be interpreted as `DEFAULT`, and `Some(None)` will be
/// interpreted as `NULL`.
pub fn update_from<T>(table: T) -> UpdateFromStatement<T>
where
    T: Table,
{
    UpdateFromStatement {
        table: table.from_clause(),
        set_clause: SetNotCalled,
        values: ValuesNotCalled,
        where_clause: WhereNotCalled,
    }
}
