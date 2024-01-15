// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{fmt, ops::Deref, vec};

use async_graphql::{
    connection::{CursorType, OpaqueCursor},
    *,
};
use diesel::{
    query_builder::QueryFragment, query_dsl::LoadQuery, QueryDsl, QueryResult, QuerySource,
};
use serde::{de::DeserializeOwned, Serialize};

use crate::{
    config::ServiceConfig,
    data::{Conn, DbConnection, DieselBackend, DieselConn, Query},
    error::Error,
};

/// Wrap the `OpaqueCursor` type to implement Scalar for it.
pub(crate) struct Cursor<C>(OpaqueCursor<C>);

/// Connection field parameters parsed into a single type that encodes the bounds of a single page
/// in a paginated response.
#[derive(Debug, Clone)]
pub(crate) struct Page<C> {
    /// The exclusive lower bound of the page (no bound means start from the beginning of the
    /// data-set).
    after: Option<Cursor<C>>,

    /// The exclusive upper bound of the page (no bound means continue to the end of the data-set).
    before: Option<Cursor<C>>,

    /// Maximum number of entries in the page.
    limit: u64,

    /// In case there are more than `limit` entries in the range described by `(after, before)`,
    /// this field states whether the entries up to limit are taken fron the `Front` or `Back` of
    /// that range.
    end: End,
}

/// Whether the page is extracted from the beginning or the end of the range bounded by the cursors.
#[derive(PartialEq, Eq, Debug, Clone, Copy)]
enum End {
    Front,
    Back,
}

/// Results from the database that are pointed to by cursors.
pub(crate) trait Target<C> {
    type Source: QuerySource;

    /// Adds a filter to `query` to bound its result to be greater than or equal to `cursor`
    /// (returning the new query).
    fn filter_ge<ST, GB>(
        cursor: &C,
        query: Query<ST, Self::Source, GB>,
    ) -> Query<ST, Self::Source, GB>;

    /// Adds a filter to `query` to bound its results to be less than or equal to `cursor`
    /// (returning the new query).
    fn filter_le<ST, GB>(
        cursor: &C,
        query: Query<ST, Self::Source, GB>,
    ) -> Query<ST, Self::Source, GB>;

    /// Adds an `ORDER BY` clause to `query` to order rows according to their cursor values
    /// (returning the new query). The `asc` parameter controls whether the ordering is ASCending
    /// (`true`) or descending (`false`).
    fn order<ST, GB>(asc: bool, query: Query<ST, Self::Source, GB>) -> Query<ST, Self::Source, GB>;

    /// The cursor pointing at this target value.
    fn cursor(&self) -> C;
}

impl<C> Cursor<C> {
    pub(crate) fn new(cursor: C) -> Self {
        Cursor(OpaqueCursor(cursor))
    }
}

impl<C> Page<C> {
    /// Convert connection parameters into a page. Entries for the page are drawn from the range
    /// `(after, before)` (Both bounds are optional). The number of entries in the page is
    /// controlled by `first` and `last`.
    ///
    /// - Setting both is in an error.
    /// - Setting `first` indicates that the entries are taken from the front of the range.
    /// - Setting `last` indicates that the entries are taken from the end of the range.
    /// - Setting neither defaults the limit to the default page size in `config`, taken from the
    ///   front of the range.
    ///
    /// It is an error to set a limit on page size that is greater than the `config`'s max page
    /// size.
    pub(crate) fn from_params(
        config: &ServiceConfig,
        first: Option<u64>,
        after: Option<Cursor<C>>,
        last: Option<u64>,
        before: Option<Cursor<C>>,
    ) -> Result<Self> {
        let limits = &config.limits;
        let page = match (first, after, last, before) {
            (Some(_), _, Some(_), _) => return Err(Error::CursorNoFirstLast.extend()),

            (limit, after, None, before) => Page {
                after,
                before,
                limit: limit.unwrap_or(limits.default_page_size),
                end: End::Front,
            },

            (None, after, Some(limit), before) => Page {
                after,
                before,
                limit,
                end: End::Back,
            },
        };

        if page.limit > limits.max_page_size {
            return Err(Error::PageTooLarge(page.limit, limits.max_page_size).extend());
        }

        Ok(page)
    }

    pub(crate) fn after(&self) -> Option<&C> {
        self.after.as_deref()
    }

    pub(crate) fn before(&self) -> Option<&C> {
        self.before.as_deref()
    }

    pub(crate) fn limit(&self) -> usize {
        self.limit as usize
    }

    pub(crate) fn is_from_front(&self) -> bool {
        matches!(self.end, End::Front)
    }
}

impl Page<usize> {
    /// Treat the cursors of this Page as indices into a range [0, total). Returns two booleans
    /// indicating whether there is a previous or next page in the range, followed by an iterator of
    /// cursors within that Page.
    pub(crate) fn paginate_indices(
        &self,
        total: usize,
    ) -> Option<(bool, bool, impl Iterator<Item = Cursor<usize>>)> {
        let mut lo = self.after().map_or(0, |a| *a + 1);
        let mut hi = self.before().map_or(total, |b| *b);

        if hi <= lo {
            return None;
        } else if (hi - lo) > self.limit() {
            if self.is_from_front() {
                hi = lo + self.limit();
            } else {
                lo = hi - self.limit();
            }
        }

        Some((0 < lo, hi < total, (lo..hi).map(Cursor::new)))
    }
}

impl<C: Eq + Clone + Send + Sync + 'static> Page<C> {
    /// Treat the cursors of this page as upper- and lowerbound filters for a database `query`.
    /// Returns two booleans indicating whether there is a previous or next page in the range,
    /// followed by an iterator of values in the page, fetched from the database.
    ///
    /// The values returned implement `Target<C>`, so are able to compute their own cursors.
    pub(crate) fn paginate_query<T, Q, ST, GB>(
        &self,
        conn: &mut Conn<'_>,
        query: Q,
    ) -> QueryResult<(bool, bool, impl Iterator<Item = T>)>
    where
        Q: Fn() -> Query<ST, T::Source, GB>,
        Query<ST, T::Source, GB>: LoadQuery<'static, DieselConn, T>,
        Query<ST, T::Source, GB>: QueryFragment<DieselBackend>,
        <T as Target<C>>::Source: Send + 'static,
        <<T as Target<C>>::Source as QuerySource>::FromClause: Send + 'static,
        Q: Send + 'static,
        T: Send + Target<C> + 'static,
        ST: Send + 'static,
        GB: Send + 'static,
    {
        let page = self.clone();
        let query = move || {
            let mut query = query();
            if let Some(after) = page.after() {
                query = T::filter_ge(after, query);
            }

            if let Some(before) = page.before() {
                query = T::filter_le(before, query);
            }

            // Load extra rows to detect the existence of pages on either side.
            query = query.limit(page.limit() as i64 + 2);
            T::order(page.is_from_front(), query)
        };

        let results: Vec<T> = if self.limit() == 0 {
            // Avoid the database roundtrip in the degenerate case.
            vec![]
        } else {
            let mut results = conn.results(query)?;
            if !self.is_from_front() {
                results.reverse();
            }
            results
        };

        // Detect whether the results imply the existence of a previous or next page.
        let (prev, next, prefix, suffix) = match (
            self.after(),
            results.first(),
            results.last(),
            self.before(),
            self.end,
        ) {
            // Results came back empty, despite supposedly including the `after` and `before`
            // cursors, so the bounds must have been invalid, no matter which end the page was
            // drawn from.
            (_, None, _, _, _) | (_, _, None, _, _) => {
                return Ok((false, false, vec![].into_iter()));
            }

            // Page drawn from the front, and the cursor for the first element does not match
            // `after`. This implies the bound was invalid, so we return an empty result.
            (Some(a), Some(f), _, _, End::Front) if f.cursor() != *a => {
                return Ok((false, false, vec![].into_iter()));
            }

            // Similar to above case, but for back of results.
            (_, _, Some(l), Some(b), End::Back) if l.cursor() != *b => {
                return Ok((false, false, vec![].into_iter()));
            }

            // From here onwards, we know that the results are non-empty and if a cursor was
            // supplied on the end the page is being drawn from, it was found in the results
            // (implying a page follows in that direction).

            // If both cursors are provided, and match both edges of the results, then we are in a
            // special case where the limit, or the end of the page being drawn from do not matter,
            // because the subsequence defined by the cursors is smaller than the limit.
            (Some(a), Some(f), Some(l), Some(b), _) if f.cursor() == *a && l.cursor() == *b => {
                (true, true, 1, 1)
            }

            // From here onwards, to detect whether there is a page on the other side than the page
            // is being drawn from, it is enough to check the length of the results.
            (after, _, _, _, End::Front) => {
                let has_previous_page = after.is_some();
                let prefix = has_previous_page as usize;
                let suffix = results.len() - results.len().min(self.limit() + prefix);
                let has_next_page = suffix > 0;

                (has_previous_page, has_next_page, prefix, suffix)
            }

            (_, _, _, before, End::Back) => {
                let has_next_page = before.is_some();
                let suffix = has_next_page as usize;
                let prefix = results.len() - results.len().min(self.limit() + suffix);
                let has_previous_page = prefix > 0;

                (has_previous_page, has_next_page, prefix, suffix)
            }
        };

        // If after trimming, we're going to return no elements, then forget whether there's a
        // previous or next page, because there will be no start or end cursor for this page to
        // anchor on.
        if results.len() == prefix + suffix {
            return Ok((false, false, vec![].into_iter()));
        }

        // We finally made it -- trim the prefix and suffix rows from the result and send it!
        let mut results = results.into_iter();
        if prefix > 0 {
            results.nth(prefix - 1);
        }
        if suffix > 0 {
            results.nth_back(suffix - 1);
        }

        Ok((prev, next, results))
    }
}

#[Scalar(name = "String", visible = false)]
impl<C> ScalarType for Cursor<C>
where
    C: Send + Sync,
    C: Serialize + DeserializeOwned,
{
    fn parse(value: Value) -> InputValueResult<Self> {
        let Value::String(s) = value else {
            return Err(InputValueError::expected_type(value));
        };

        Ok(Cursor(OpaqueCursor::decode_cursor(&s)?))
    }

    /// Just check that the value is a string, as we'll do more involved tests during parsing.
    fn is_valid(value: &Value) -> bool {
        matches!(value, Value::String(_))
    }

    fn to_value(&self) -> Value {
        Value::String(self.0.encode_cursor())
    }
}

/// Wrapping implementation of `CursorType` directly forwarding to `OpaqueCursor`.
impl<C> CursorType for Cursor<C>
where
    C: Send + Sync,
    C: Serialize + DeserializeOwned,
{
    type Error = <OpaqueCursor<C> as CursorType>::Error;

    fn decode_cursor(s: &str) -> Result<Self, Self::Error> {
        Ok(Cursor(OpaqueCursor::decode_cursor(s)?))
    }

    fn encode_cursor(&self) -> String {
        self.0.encode_cursor()
    }
}

impl<C> Deref for Cursor<C> {
    type Target = C;

    fn deref(&self) -> &Self::Target {
        self.0.deref()
    }
}

impl<C: fmt::Debug> fmt::Debug for Cursor<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", *self.0)
    }
}

impl<C: Clone> Clone for Cursor<C> {
    fn clone(&self) -> Self {
        Cursor::new(self.0 .0.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn test_default_page() {
        let config = ServiceConfig::default();
        let page: Page<u64> = Page::from_params(&config, None, None, None, None).unwrap();

        let expect = expect![[r#"
            Page {
                after: None,
                before: None,
                limit: 20,
                end: Front,
            }"#]];
        expect.assert_eq(&format!("{page:#?}"));
    }

    #[test]
    fn test_prefix_page() {
        let config = ServiceConfig::default();
        let page: Page<u64> =
            Page::from_params(&config, None, Some(Cursor::new(42)), None, None).unwrap();

        let expect = expect![[r#"
            Page {
                after: Some(
                    42,
                ),
                before: None,
                limit: 20,
                end: Front,
            }"#]];
        expect.assert_eq(&format!("{page:#?}"));
    }

    #[test]
    fn test_prefix_page_limited() {
        let config = ServiceConfig::default();
        let page: Page<u64> =
            Page::from_params(&config, Some(10), Some(Cursor::new(42)), None, None).unwrap();

        let expect = expect![[r#"
            Page {
                after: Some(
                    42,
                ),
                before: None,
                limit: 10,
                end: Front,
            }"#]];
        expect.assert_eq(&format!("{page:#?}"));
    }

    #[test]
    fn test_suffix_page() {
        let config = ServiceConfig::default();
        let page: Page<u64> =
            Page::from_params(&config, None, None, None, Some(Cursor::new(42))).unwrap();

        let expect = expect![[r#"
            Page {
                after: None,
                before: Some(
                    42,
                ),
                limit: 20,
                end: Front,
            }"#]];
        expect.assert_eq(&format!("{page:#?}"));
    }

    #[test]
    fn test_suffix_page_limited() {
        let config = ServiceConfig::default();
        let page: Page<u64> =
            Page::from_params(&config, None, None, Some(10), Some(Cursor::new(42))).unwrap();

        let expect = expect![[r#"
            Page {
                after: None,
                before: Some(
                    42,
                ),
                limit: 10,
                end: Back,
            }"#]];
        expect.assert_eq(&format!("{page:#?}"));
    }

    #[test]
    fn test_between_page_prefix() {
        let config = ServiceConfig::default();
        let page: Page<u64> = Page::from_params(
            &config,
            Some(10),
            Some(Cursor::new(40)),
            None,
            Some(Cursor::new(42)),
        )
        .unwrap();

        let expect = expect![[r#"
            Page {
                after: Some(
                    40,
                ),
                before: Some(
                    42,
                ),
                limit: 10,
                end: Front,
            }"#]];
        expect.assert_eq(&format!("{page:#?}"));
    }

    #[test]
    fn test_between_page_suffix() {
        let config = ServiceConfig::default();
        let page: Page<u64> = Page::from_params(
            &config,
            None,
            Some(Cursor::new(40)),
            Some(10),
            Some(Cursor::new(42)),
        )
        .unwrap();

        let expect = expect![[r#"
            Page {
                after: Some(
                    40,
                ),
                before: Some(
                    42,
                ),
                limit: 10,
                end: Back,
            }"#]];
        expect.assert_eq(&format!("{page:#?}"));
    }

    #[test]
    fn test_between_page() {
        let config = ServiceConfig::default();
        let page: Page<u64> = Page::from_params(
            &config,
            None,
            Some(Cursor::new(40)),
            None,
            Some(Cursor::new(42)),
        )
        .unwrap();

        let expect = expect![[r#"
            Page {
                after: Some(
                    40,
                ),
                before: Some(
                    42,
                ),
                limit: 20,
                end: Front,
            }"#]];
        expect.assert_eq(&format!("{page:#?}"));
    }

    #[test]
    fn test_err_first_and_last() {
        let config = ServiceConfig::default();
        let err = Page::<u64>::from_params(&config, Some(1), None, Some(1), None).unwrap_err();

        let expect = expect![[r#"
            Error {
                message: "'first' and 'last' must not be used together",
                extensions: Some(
                    ErrorExtensionValues(
                        {
                            "code": String(
                                "BAD_USER_INPUT",
                            ),
                        },
                    ),
                ),
            }"#]];
        expect.assert_eq(&format!("{err:#?}"));
    }

    #[test]
    fn test_err_page_too_big() {
        let config = ServiceConfig::default();
        let too_big = config.limits.max_page_size + 1;
        let err = Page::<u64>::from_params(&config, Some(too_big), None, None, None).unwrap_err();

        let expect = expect![[r#"
            Error {
                message: "Connection's page size of 51 exceeds max of 50",
                extensions: Some(
                    ErrorExtensionValues(
                        {
                            "code": String(
                                "BAD_USER_INPUT",
                            ),
                        },
                    ),
                ),
            }"#]];
        expect.assert_eq(&format!("{err:#?}"));
    }
}
