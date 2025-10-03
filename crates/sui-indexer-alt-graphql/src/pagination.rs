// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use async_graphql::{
    connection::{Connection, CursorType, Edge},
    registry::MetaField,
    OutputType,
};
use sui_pg_db::query::Query;
use sui_sql_macro::query;

use crate::api::scalars::cursor::JsonCursor;

/// Configuration for page size limits, specifying a max multi-get size, as well as a default and
/// max page size for each paginated fields. Page limits can be customized for specific fields,
/// otherwise falling back to a blanket default.
pub(crate) struct PaginationConfig {
    /// Maximum number of keys that can be fetched in a single multi-get.
    max_multi_get_size: u32,

    /// Fallback page limit configuration.
    fallback: PageLimits,

    /// Type and field name-specific overrides for page limits.
    overrides: BTreeMap<(&'static str, &'static str), PageLimits>,
}

/// The configuration for a single paginated field.
pub(crate) struct PageLimits {
    pub(crate) default: u32,
    pub(crate) max: u32,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct Page<C> {
    /// The exclusive lower bound of the page (no bound means start from the beginning of the
    /// data-set).
    after: Option<C>,

    /// The exclusive upper bound of the page (no bound means continue to the end of the data-set).
    before: Option<C>,

    /// Maximum number of entries in the page.
    limit: u64,

    /// In case there are more than `limit` entries in the range described by `(after, before)`,
    /// this field states whether the entries up to limit are taken from the `Front` or `Back` of
    /// that range.
    end: End,
}

/// Whether the page is extracted from the beginning or the end of the range bounded by the cursors.
#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub(crate) enum End {
    Front,
    Back,
}

#[derive(thiserror::Error, Debug, Clone)]
pub(crate) enum Error {
    #[error("Cannot provide both 'first' and 'last' parameters for connection")]
    FirstAndLast,

    #[error("Page size of {limit} exceeds max of {max} for connection")]
    TooLarge { limit: u64, max: u32 },
}

impl PaginationConfig {
    pub(crate) fn new(
        max_multi_get_size: u32,
        fallback: PageLimits,
        overrides: BTreeMap<(&'static str, &'static str), PageLimits>,
    ) -> Self {
        Self {
            max_multi_get_size,
            fallback,
            overrides,
        }
    }

    /// Maximum number of keys that can be fetched in a single multi-get.
    pub(crate) fn max_multi_get_size(&self) -> u32 {
        self.max_multi_get_size
    }

    /// Fetch the default and max page size for this type and field.
    pub(crate) fn limits<'l, 't: 'l>(&'l self, type_: &'t str, name: &'t str) -> &'l PageLimits {
        self.overrides.get(&(type_, name)).unwrap_or(&self.fallback)
    }
}

impl PageLimits {
    /// Limits for fetching a single item.
    pub(crate) fn singleton() -> Self {
        Self { default: 1, max: 1 }
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
    /// - Setting neither defaults the limit to the default page size in `limits`, taken from the
    ///   front of the range.
    ///
    /// It is an error to set a limit on page size that is greater than the `limit`'s max page
    /// size.
    pub(crate) fn from_params(
        limits: &PageLimits,
        first: Option<u64>,
        after: Option<C>,
        last: Option<u64>,
        before: Option<C>,
    ) -> Result<Self, Error> {
        let page = match (first, after, last, before) {
            (Some(_), _, Some(_), _) => return Err(Error::FirstAndLast),

            (limit, after, None, before) => Page {
                after,
                before,
                limit: limit.unwrap_or(limits.default as u64),
                end: End::Front,
            },

            (None, after, Some(limit), before) => Page {
                after,
                before,
                limit,
                end: End::Back,
            },
        };

        if page.limit > limits.max as u64 {
            return Err(Error::TooLarge {
                limit: page.limit,
                max: limits.max,
            });
        }

        Ok(page)
    }

    pub(crate) fn after(&self) -> Option<&C> {
        self.after.as_ref()
    }

    pub(crate) fn before(&self) -> Option<&C> {
        self.before.as_ref()
    }

    pub(crate) fn limit(&self) -> usize {
        self.limit as usize
    }

    /// Returns the limit + 2 for has_previous_page and has_next_page pagination calculations
    pub(crate) fn limit_with_overhead(&self) -> usize {
        self.limit() + 2
    }

    pub(crate) fn is_from_front(&self) -> bool {
        matches!(self.end, End::Front)
    }

    /// Direction for sorting SQL queries.
    pub(crate) fn order_by_direction(&self) -> Query {
        if self.is_from_front() {
            query!("ASC")
        } else {
            query!("DESC")
        }
    }
}

impl Page<JsonCursor<usize>> {
    /// Treat the cursors of this Page as indices into a range [0, total).
    ///
    /// Returns a connection where the cursors correspond to a sub-range of indices and the
    /// nodes are selected by calling `node` with each index in that sub-range.
    pub(crate) fn paginate_indices<N: OutputType, E>(
        &self,
        total: usize,
        node: impl Fn(usize) -> Result<N, E>,
    ) -> Result<Connection<String, N>, E> {
        let mut lo = self.after().map_or(0, |a| a.saturating_add(1));
        let mut hi = self.before().map_or(total, |b| **b);
        let mut conn = Connection::new(false, false);

        if hi <= lo {
            return Ok(conn);
        } else if (hi - lo) > self.limit() {
            if self.is_from_front() {
                hi = lo + self.limit();
            } else {
                lo = hi - self.limit();
            }
        }

        conn.has_previous_page = 0 < lo;
        conn.has_next_page = hi < total;
        for i in lo..hi {
            conn.edges
                .push(Edge::new(JsonCursor::new(i).encode_cursor(), node(i)?));
        }

        Ok(conn)
    }
}

impl<C: CursorType + Eq + PartialEq + Clone> Page<C> {
    /// Process the results of a paginated query.
    ///
    /// `results` is expected to be the result of a query modified to fetch the page indicated by
    /// `self`. This function determines whether those results are consistent with the cursors for
    /// this page, and whether there are more results in the set being paginated, before or after
    /// the current page.
    ///
    /// `results` should contain the elements of the page, in order, as well as at least one record
    /// either side of the page, if there is one.
    ///
    /// `cursor` is a function that extracts the cursor from an element of `results`, and `node` is
    /// a function that extracts the node. Returns a GraphQL `Connection` populated with edges
    /// derived from `results`.
    pub(crate) fn paginate_results<T, N: OutputType, E>(
        &self,
        results: Vec<T>,
        cursor: impl Fn(&T) -> C,
        node: impl Fn(T) -> Result<N, E>,
    ) -> Result<Connection<String, N>, E> {
        let edges: Vec<_> = results.into_iter().map(|r| (cursor(&r), r)).collect();
        let first = edges.first().map(|(c, _)| c.clone());
        let last = edges.last().map(|(c, _)| c.clone());

        let (prev, next, prefix, suffix) =
            match (self.after(), first, last, self.before(), self.end) {
                // Results came back empty, despite supposedly including the `after` and `before`
                // cursors, so the bounds must have been invalid, no matter which end the page was
                // drawn from.
                (_, None, _, _, _) | (_, _, None, _, _) => {
                    return Ok(Connection::new(false, false));
                }

                // Page drawn from the front, and the cursor for the first element does not match
                // `after`. This absence implies the bound was invalid, so we return an empty
                // result.
                (Some(a), Some(f), _, _, End::Front) if f != *a => {
                    return Ok(Connection::new(false, false));
                }

                // Similar to above case, but for back of results.
                (_, _, Some(l), Some(b), End::Back) if l != *b => {
                    return Ok(Connection::new(false, false));
                }

                // From here onwards, we know that the results are non-empty. In the forward
                // pagination scenario, the presence of a previous page is determined by whether a
                // cursor supplied on the end the page is being drawn from is found in the first
                // position. The presence of a next page is determined by whether we have more
                // results than the provided limit, and/ or if the end cursor element appears in
                // the result set.
                (after, Some(f), Some(l), before, End::Front) => {
                    let has_previous_page = after.is_some_and(|a| *a == f);
                    let prefix = has_previous_page as usize;

                    // If results end with the before cursor, we will at least need to trim one
                    // element from the suffix and we trim more off the end if there is more after
                    // applying the limit.
                    let mut suffix = before.is_some_and(|b| *b == l) as usize;
                    suffix += edges.len().saturating_sub(self.limit() + prefix + suffix);
                    let has_next_page = suffix > 0;

                    (has_previous_page, has_next_page, prefix, suffix)
                }

                // Symmetric to the previous case, but drawing from the back.
                (after, Some(f), Some(l), before, End::Back) => {
                    let has_next_page = before.is_some_and(|b| *b == l);
                    let suffix = has_next_page as usize;

                    let mut prefix = after.is_some_and(|a| *a == f) as usize;
                    prefix += edges.len().saturating_sub(self.limit() + prefix + suffix);
                    let has_previous_page = prefix > 0;

                    (has_previous_page, has_next_page, prefix, suffix)
                }
            };

        // If there are no elements left after trimming, then forget whether there's a previous or
        // next page, because there will be no start or end cursor for this page to anchor on.
        if edges.len() == prefix + suffix {
            return Ok(Connection::new(false, false));
        }

        // We finally made it -- trim the prefix and suffix edges from the result and send it!
        let mut edges = edges.into_iter();
        if prefix > 0 {
            edges.nth(prefix - 1);
        }
        if suffix > 0 {
            edges.nth_back(suffix - 1);
        }

        let mut conn = Connection::new(prev, next);
        for (c, n) in edges {
            conn.edges.push(Edge::new(c.encode_cursor(), node(n)?));
        }

        Ok(conn)
    }
}

/// Decides whether the field's return type is paginated.
pub(crate) fn is_connection(field: &MetaField) -> bool {
    let type_ = field.ty.as_str();
    type_.ends_with("Connection") || type_.ends_with("Connection!")
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;

    use super::*;

    // Helper to create a node - identity function that can't fail
    fn node<N>(n: N) -> Result<N, Infallible> {
        Ok(n)
    }

    // Helper to create a (cursor, node) tuple for test expectations
    fn edge(value: usize) -> (String, usize) {
        (value.encode_cursor(), value)
    }

    #[test]
    fn test_default_page() {
        let limits = PageLimits {
            default: 10,
            max: 100,
        };

        assert_eq!(
            Page::<usize>::from_params(&limits, None, None, None, None).unwrap(),
            Page {
                after: None,
                before: None,
                limit: limits.default as u64,
                end: End::Front
            }
        );
    }

    #[test]
    fn test_page_forward() {
        let limits = PageLimits {
            default: 10,
            max: 100,
        };

        assert_eq!(
            Page::from_params(&limits, None, Some(1), None, None).unwrap(),
            Page {
                after: Some(1),
                before: None,
                limit: limits.default as u64,
                end: End::Front
            }
        );

        // Even if you provide a `before` cursor, nodes are still fetched from the front of the
        // range if `last` is not specified.
        assert_eq!(
            Page::from_params(&limits, None, None, None, Some(10)).unwrap(),
            Page {
                after: None,
                before: Some(10),
                limit: limits.default as u64,
                end: End::Front
            }
        );

        assert_eq!(
            Page::from_params(&limits, Some(5), Some(1), None, None).unwrap(),
            Page {
                after: Some(1),
                before: None,
                limit: 5,
                end: End::Front
            }
        );
    }

    #[test]
    fn test_page_backward() {
        let limits = PageLimits {
            default: 10,
            max: 100,
        };

        assert_eq!(
            Page::from_params(&limits, None, None, Some(5), Some(10)).unwrap(),
            Page {
                after: None,
                before: Some(10),
                limit: 5,
                end: End::Back
            }
        );
    }

    #[test]
    fn test_page_both() {
        let limits = PageLimits {
            default: 10,
            max: 100,
        };

        assert!(matches!(
            Page::<usize>::from_params(&limits, Some(5), None, Some(5), None),
            Err(Error::FirstAndLast)
        ));
    }

    #[test]
    fn test_page_too_large() {
        let limits = PageLimits {
            default: 10,
            max: 100,
        };

        assert!(matches!(
            Page::<usize>::from_params(&limits, Some(1000), None, None, None),
            Err(Error::TooLarge {
                limit: 1000,
                max: 100,
            })
        ));
    }

    #[test]
    fn test_paginate_results() {
        let limits = PageLimits {
            default: 5,
            max: 100,
        };

        let page: Page<usize> = Page::from_params(&limits, None, None, None, None).unwrap();
        let results = vec![0, 1, 2, 3, 4];
        let expect = vec![edge(0), edge(1), edge(2), edge(3), edge(4)];

        let conn = page.paginate_results(results, |r| *r, node).unwrap();
        let actual: Vec<(String, usize)> =
            conn.edges.into_iter().map(|e| (e.cursor, e.node)).collect();

        assert!(!conn.has_previous_page);
        assert!(!conn.has_next_page);
        assert_eq!(expect, actual);
    }

    #[test]
    fn test_paginate_results_not_enough() {
        let limits = PageLimits {
            default: 5,
            max: 10,
        };

        let page: Page<usize> = Page::from_params(&limits, None, None, None, None).unwrap();

        let results = vec![0, 1, 2];
        let expect = vec![edge(0), edge(1), edge(2)];

        let conn = page.paginate_results(results, |r| *r, node).unwrap();
        let actual: Vec<(String, usize)> =
            conn.edges.into_iter().map(|e| (e.cursor, e.node)).collect();

        assert!(!conn.has_previous_page);
        assert!(!conn.has_next_page);
        assert_eq!(expect, actual);
    }

    #[test]
    fn test_paginate_results_limited() {
        let limits = PageLimits {
            default: 5,
            max: 10,
        };

        let page: Page<usize> = Page::from_params(&limits, None, None, None, None).unwrap();
        let results = vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9];
        let expect = vec![edge(0), edge(1), edge(2), edge(3), edge(4)];

        let conn = page.paginate_results(results, |r| *r, node).unwrap();
        let actual: Vec<(String, usize)> =
            conn.edges.into_iter().map(|e| (e.cursor, e.node)).collect();

        assert!(!conn.has_previous_page);
        assert!(conn.has_next_page);
        assert_eq!(expect, actual);
    }

    #[test]
    fn test_paginate_results_backward_limited() {
        let limits = PageLimits {
            default: 5,
            max: 10,
        };

        let page: Page<usize> =
            Page::from_params(&limits, None, None, Some(limits.default as u64), None).unwrap();

        let results = vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9];
        let expect = vec![edge(5), edge(6), edge(7), edge(8), edge(9)];

        let conn = page.paginate_results(results, |r| *r, node).unwrap();
        let actual: Vec<(String, usize)> =
            conn.edges.into_iter().map(|e| (e.cursor, e.node)).collect();

        assert!(conn.has_previous_page);
        assert!(!conn.has_next_page);
        assert_eq!(expect, actual);
    }

    #[test]
    fn test_paginate_results_with_cursors() {
        let limits = PageLimits {
            default: 5,
            max: 10,
        };

        let page: Page<usize> = Page::from_params(&limits, Some(5), Some(2), None, None).unwrap();

        let results = vec![2, 3, 4, 5, 6, 7, 8, 9];
        let expect = vec![edge(3), edge(4), edge(5), edge(6), edge(7)];

        let conn = page.paginate_results(results, |r| *r, node).unwrap();
        let actual: Vec<(String, usize)> =
            conn.edges.into_iter().map(|e| (e.cursor, e.node)).collect();

        assert!(conn.has_previous_page);
        assert!(conn.has_next_page);
        assert_eq!(expect, actual);
    }

    #[test]
    fn test_paginate_results_inconsistent_cursor() {
        let limits = PageLimits {
            default: 5,
            max: 10,
        };

        let page: Page<usize> = Page::from_params(&limits, None, Some(2), None, None).unwrap();

        let results = vec![4, 5];
        let expect: Vec<(String, usize)> = vec![];

        let conn = page.paginate_results(results, |r| *r, node).unwrap();
        let actual: Vec<(String, usize)> =
            conn.edges.into_iter().map(|e| (e.cursor, e.node)).collect();

        assert!(!conn.has_previous_page);
        assert!(!conn.has_next_page);
        assert_eq!(expect, actual);
    }

    #[test]
    fn test_paginate_results_empty() {
        let limits = PageLimits {
            default: 5,
            max: 10,
        };

        let page: Page<usize> = Page::from_params(&limits, None, Some(2), None, Some(3)).unwrap();

        let results = vec![2, 3];
        let expect: Vec<(String, usize)> = vec![];

        let conn = page.paginate_results(results, |r| *r, node).unwrap();
        let actual: Vec<(String, usize)> =
            conn.edges.into_iter().map(|e| (e.cursor, e.node)).collect();

        assert!(!conn.has_previous_page);
        assert!(!conn.has_next_page);
        assert_eq!(expect, actual);
    }
}
