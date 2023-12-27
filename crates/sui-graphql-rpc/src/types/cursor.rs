// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{fmt, ops::Deref};

use async_graphql::{
    connection::{CursorType, OpaqueCursor},
    *,
};
use serde::{de::DeserializeOwned, Serialize};

use crate::{config::ServiceConfig, error::Error};

/// Wrap the `OpaqueCursor` type to implement Scalar for it.
pub(crate) struct Cursor<C>(OpaqueCursor<C>);

/// Connection field parameters parsed into a single type that encodes the bounds of a single page
/// in a paginated response.
#[derive(Debug)]
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
#[derive(PartialEq, Eq, Debug)]
enum End {
    Front,
    Back,
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
