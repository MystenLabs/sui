// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use async_graphql::{
    connection::{Connection, Edge, EmptyFields},
    registry::MetaField,
};

use crate::{
    api::scalars::cursor::JsonCursor,
    consistency::{self, Checkpointed, Indexed},
    scope::Scope,
};

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
    pub default: u32,
    pub max: u32,
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

    #[error(
        "'after' cursor is pinned to checkpoint {after} and 'before' cursor is pinned to \
         checkpoint {before}. They cannot be used together."
    )]
    InconsistentCheckpoint { after: u64, before: u64 },

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

    pub(crate) fn is_from_front(&self) -> bool {
        matches!(self.end, End::Front)
    }
}

impl<C: Checkpointed> Page<C> {
    /// Calculate the checkpoint that results under this page should be pinned to, based on
    /// checkpoint information in the cursors, or if none are provided, the scope that the page is
    /// being fetched in.
    ///
    /// It is an error for both cursors to be present and pointing to pinned to different
    /// checkpoints.
    fn checkpoint_viewed_at(&self, scope: &Scope) -> Result<u64, Error> {
        match (self.after(), self.before()) {
            // If both cursors have been supplied, they must be pinned to the same checkpoint.
            (Some(a), Some(b)) if a.checkpoint_viewed_at() != b.checkpoint_viewed_at() => {
                Err(Error::InconsistentCheckpoint {
                    after: a.checkpoint_viewed_at(),
                    before: b.checkpoint_viewed_at(),
                })
            }

            // Otherwise, either cursor sets the checkpoint viewed at.
            (Some(c), _) | (_, Some(c)) => Ok(c.checkpoint_viewed_at()),

            // Otherwise, it is set by the overall scope.
            (None, None) => Ok(scope.checkpoint_viewed_at()),
        }
    }
}

impl Page<JsonCursor<consistency::Indexed>> {
    /// Treat the cursors of this Page as indices into a range [0, total). Returns a connection
    /// with the cursors filled out, but no data.
    ///
    /// `scope` is used to pin the returned cursors to a checkpoint in case the page contains no
    /// cursors to get consistency information from.
    pub(crate) fn paginate_indices(
        &self,
        scope: &Scope,
        total: usize,
    ) -> Result<Connection<JsonCursor<consistency::Indexed>, EmptyFields>, Error> {
        let checkpoint_viewed_at = self.checkpoint_viewed_at(scope)?;

        let mut lo = self.after().map_or(0, |a| a.ix.saturating_add(1));
        let mut hi = self.before().map_or(total, |b| b.ix);
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
        for ix in lo..hi {
            conn.edges.push(Edge::new(
                JsonCursor::new(Indexed {
                    ix,
                    checkpoint_viewed_at,
                }),
                EmptyFields,
            ));
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
    use serde::{Deserialize, Serialize};

    use super::*;

    #[derive(Serialize, Deserialize)]
    struct Consistent(usize);

    impl Checkpointed for JsonCursor<Consistent> {
        fn checkpoint_viewed_at(&self) -> u64 {
            self.0 as u64
        }
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
    fn test_page_consistency_with_cursors() {
        let limits = PageLimits {
            default: 10,
            max: 100,
        };

        let scope = Scope::new_for_test(10);
        let page: Page<JsonCursor<Consistent>> = Page::from_params(
            &limits,
            None,
            Some(JsonCursor::new(Consistent(5))),
            None,
            Some(JsonCursor::new(Consistent(5))),
        )
        .unwrap();

        assert_eq!(5, page.checkpoint_viewed_at(&scope).unwrap());
    }

    #[test]
    fn test_page_consistency_with_one_cursor() {
        let limits = PageLimits {
            default: 10,
            max: 100,
        };

        let scope = Scope::new_for_test(10);

        // `after` cursor only.
        let page: Page<JsonCursor<Consistent>> = Page::from_params(
            &limits,
            None,
            Some(JsonCursor::new(Consistent(5))),
            None,
            None,
        )
        .unwrap();

        assert_eq!(5, page.checkpoint_viewed_at(&scope).unwrap());

        // `before` cursor only.
        let page: Page<JsonCursor<Consistent>> = Page::from_params(
            &limits,
            None,
            None,
            None,
            Some(JsonCursor::new(Consistent(5))),
        )
        .unwrap();

        assert_eq!(5, page.checkpoint_viewed_at(&scope).unwrap());
    }

    #[test]
    fn test_page_consistency_with_no_cursor() {
        let limits = PageLimits {
            default: 10,
            max: 100,
        };

        let scope = Scope::new_for_test(10);
        let page: Page<JsonCursor<Consistent>> =
            Page::from_params(&limits, None, None, None, None).unwrap();

        assert_eq!(10, page.checkpoint_viewed_at(&scope).unwrap());
    }

    #[test]
    fn test_page_inconsistent() {
        let limits = PageLimits {
            default: 10,
            max: 100,
        };

        let scope = Scope::new_for_test(10);
        let page: Page<JsonCursor<Consistent>> = Page::from_params(
            &limits,
            None,
            Some(JsonCursor::new(Consistent(5))),
            None,
            Some(JsonCursor::new(Consistent(6))),
        )
        .unwrap();

        assert!(matches!(
            page.checkpoint_viewed_at(&scope).unwrap_err(),
            Error::InconsistentCheckpoint {
                after: 5,
                before: 6,
            }
        ));
    }
}
