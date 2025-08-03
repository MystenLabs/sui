// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context;
use bincode::{Decode, Encode};
use serde::{de::DeserializeOwned, Serialize};
use sui_default_config::DefaultConfig;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::End;

use crate::db::{
    iter::{FwdIter, RevIter},
    map::DbMap,
};

use super::error::{db_error, RpcError};

#[DefaultConfig]
pub struct PaginationConfig {
    pub default_page_size: u32,
    pub max_batch_size: u32,
    pub max_page_size: u32,
}

pub(super) struct Page<'r> {
    after: Option<&'r [u8]>,
    before: Option<&'r [u8]>,
    limit: usize,
    is_from_front: bool,
}

/// Data for a paginated response.
#[derive(PartialEq, Eq, Debug)]
pub(super) struct Response<K, V> {
    pub has_prev: bool,
    pub has_next: bool,
    pub results: Vec<(Vec<u8>, K, V)>,
}

impl<'r> Page<'r> {
    /// Interpret fields from a gRPC request as the description of a page.
    ///
    /// If `limit` is too large, it will be clamped to the configured maximum page size. If `end`
    /// is not provided, it defaults to `End::Front`.
    pub(super) fn from_request(
        config: &PaginationConfig,
        after: &'r [u8],
        before: &'r [u8],
        limit: u32,
        end: End,
    ) -> Self {
        let limit = if limit == 0 {
            config.default_page_size
        } else if limit > config.max_page_size {
            config.max_page_size
        } else {
            limit
        } as usize;

        let is_from_front = matches!(end, End::Front | End::Unknown);

        Self {
            after: (!after.is_empty()).then_some(after),
            before: (!before.is_empty()).then_some(before),
            limit,
            is_from_front,
        }
    }

    /// Paginate over the key-value pairs in `map` that share a `prefix`, at the given
    /// `checkpoint`.
    pub(super) fn paginate_prefix<J, K, V, E>(
        &self,
        map: &DbMap<K, V>,
        checkpoint: u64,
        prefix: &J,
    ) -> Result<Response<K, V>, RpcError<E>>
    where
        J: Encode,
        K: Encode + Decode<()>,
        V: Serialize + DeserializeOwned,
    {
        self.paginate_filtered(map, checkpoint, prefix, |_, _, _| true)
    }

    /// Paginate over the key-value pairs in `map` that share a `prefix`, and match a predicate
    /// `pred`, at the given `checkpoint`.
    pub(super) fn paginate_filtered<J, K, V, E>(
        &self,
        map: &DbMap<K, V>,
        checkpoint: u64,
        prefix: &J,
        pred: impl FnMut(&[u8], &K, &V) -> bool,
    ) -> Result<Response<K, V>, RpcError<E>>
    where
        J: Encode,
        K: Encode + Decode<()>,
        V: Serialize + DeserializeOwned,
    {
        if self.is_from_front {
            self.paginate_from_front(
                map.prefix(checkpoint, prefix)
                    .map_err(|e| db_error(e, "failed to create forward iterator"))?,
                pred,
            )
        } else {
            self.paginate_from_back(
                map.prefix_rev(checkpoint, prefix)
                    .map_err(|e| db_error(e, "failed to create reverse iterator"))?,
                pred,
            )
        }
    }

    fn paginate_from_front<K, V, E>(
        &self,
        mut iter: FwdIter<'_, K, V>,
        mut pred: impl FnMut(&[u8], &K, &V) -> bool,
    ) -> Result<Response<K, V>, RpcError<E>>
    where
        K: Decode<()>,
        V: DeserializeOwned,
    {
        // Normalize the `after` cursor to handle the case where the range is empty, or the cursor
        // points outside the range. If the cursor survives this operation, there is guaranteed to
        // be a previous page.
        let after = match (iter.raw_key(), self.after) {
            (_, None) | (None, _) => None,
            (Some(f), Some(a)) => (f <= a).then_some(a),
        };

        // Seek past the cursor.
        if let Some(a) = after {
            iter.seek(a);
            if iter.raw_key() == Some(a) {
                iter.next();
            }
        }

        let mut results = Vec::with_capacity(self.limit);
        while results.len() < self.limit {
            let Some(cursor) = iter.raw_key() else {
                break;
            };

            if self.before.is_some_and(|b| cursor >= b) {
                return Ok(Response {
                    has_prev: after.is_some(),
                    has_next: true,
                    results,
                });
            }

            // SAFETY: If there is a raw key, there must be a next entry.
            let cursor = cursor.to_owned();
            let (key, value) = iter.next().unwrap().context("iteration failed")?;

            if pred(&cursor, &key, &value) {
                results.push((cursor, key, value))
            }
        }

        Ok(Response {
            has_prev: after.is_some(),
            has_next: iter.valid(),
            results,
        })
    }

    fn paginate_from_back<K, V, E>(
        &self,
        mut iter: RevIter<'_, K, V>,
        mut pred: impl FnMut(&[u8], &K, &V) -> bool,
    ) -> Result<Response<K, V>, RpcError<E>>
    where
        K: Decode<()>,
        V: DeserializeOwned,
    {
        // Normalize the `before` cursor to handle the case where the range is empty, or the cursor
        // points outside the range. If the cursor survives this operation, there is guaranteed to
        // be a next page.
        let before = match (self.before, iter.raw_key()) {
            (_, None) | (None, _) => None,
            (Some(b), Some(l)) => (b <= l).then_some(b),
        };

        // Seek past the cursor.
        if let Some(b) = before {
            iter.seek(b);
            if iter.raw_key() == Some(b) {
                iter.next();
            }
        }

        let mut results = Vec::with_capacity(self.limit);
        while results.len() < self.limit {
            let Some(cursor) = iter.raw_key() else {
                break;
            };

            if self.after.is_some_and(|a| a >= cursor) {
                results.reverse();
                return Ok(Response {
                    has_prev: true,
                    has_next: before.is_some(),
                    results,
                });
            }

            // SAFETY: If there is a raw key, there must be a next entry.
            let cursor = cursor.to_owned();
            let (key, value) = iter.next().unwrap().context("iteration failed")?;

            if pred(&cursor, &key, &value) {
                results.push((cursor, key, value))
            }
        }

        results.reverse();
        Ok(Response {
            has_prev: iter.valid(),
            has_next: before.is_some(),
            results,
        })
    }
}

impl Default for PaginationConfig {
    fn default() -> Self {
        Self {
            default_page_size: 50,
            max_batch_size: 200,
            max_page_size: 200,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{convert::Infallible, sync::Arc};

    use sui_indexer_alt_framework::store::CommitterWatermark;
    use tempfile::TempDir;

    use crate::db::{key, Db, Watermark};

    use super::*;

    fn config() -> PaginationConfig {
        PaginationConfig {
            default_page_size: 3,
            max_batch_size: 5,
            max_page_size: 5,
        }
    }

    fn wm(cp: u64) -> Watermark {
        CommitterWatermark::new_for_testing(cp).into()
    }

    fn map() -> (TempDir, DbMap<u32, u64>) {
        let d = tempfile::tempdir().unwrap();

        let mut opts = rocksdb::Options::default();
        opts.create_if_missing(true);

        let cfs = vec![("test", rocksdb::Options::default())];

        let db = Arc::new(Db::open(d.path().join("db"), opts, 4, cfs).unwrap());
        let map: DbMap<u32, u64> = DbMap::new(db.clone(), "test");

        let mut batch = rocksdb::WriteBatch::default();
        map.insert(0x0000_0001, 10, &mut batch).unwrap();
        map.insert(0x0000_0003, 30, &mut batch).unwrap();
        map.insert(0x0000_0005, 50, &mut batch).unwrap();
        map.insert(0x0000_0007, 70, &mut batch).unwrap();
        map.insert(0x0000_0009, 90, &mut batch).unwrap();
        db.write("batch", wm(0), batch).unwrap();
        db.snapshot(wm(0));

        let mut batch = rocksdb::WriteBatch::default();
        map.insert(0x0000_0000, 0, &mut batch).unwrap();
        map.insert(0x0000_0002, 20, &mut batch).unwrap();
        map.insert(0x0000_0004, 40, &mut batch).unwrap();
        map.insert(0x0000_0006, 60, &mut batch).unwrap();
        map.insert(0x0000_0008, 80, &mut batch).unwrap();
        map.insert(0x0001_0000, 1, &mut batch).unwrap();
        map.insert(0x0001_0002, 21, &mut batch).unwrap();
        db.write("batch", wm(1), batch).unwrap();
        db.snapshot(wm(1));

        (d, map)
    }

    #[test]
    fn default_page_size() {
        let page = Page::from_request(&config(), &[], &[], 0, End::Unknown);
        assert_eq!(page.limit, config().default_page_size as usize);
    }

    #[test]
    fn max_page_size() {
        let config = config();
        let page = Page::from_request(&config, &[], &[], config.max_page_size * 2, End::Unknown);
        assert_eq!(page.limit, config.max_page_size as usize);
    }

    #[test]
    fn paginate_prefix_forward() {
        let (_d, map) = map();

        let paginate = |cp: u64, prefix: u16| {
            let mut after = None;
            let mut results = vec![];
            loop {
                let cursor = after.as_deref().unwrap_or_default();
                let resp = Page::from_request(&config(), cursor, &[], 0, End::Front)
                    .paginate_prefix::<_, _, _, Infallible>(&map, cp, &prefix)
                    .unwrap();

                assert_eq!(resp.has_prev, after.is_some());
                after = resp.results.last().map(|(c, _, _)| c.clone());
                results.extend(resp.results);
                if !resp.has_next {
                    break;
                }
            }

            results
        };

        assert_eq!(
            paginate(0, 0x0000),
            vec![
                (key::encode(&0x0000_0001u32), 0x0000_0001, 10),
                (key::encode(&0x0000_0003u32), 0x0000_0003, 30),
                (key::encode(&0x0000_0005u32), 0x0000_0005, 50),
                (key::encode(&0x0000_0007u32), 0x0000_0007, 70),
                (key::encode(&0x0000_0009u32), 0x0000_0009, 90),
            ],
            "Multiple pages, at snapshot 0",
        );

        assert_eq!(
            paginate(1, 0x0000),
            vec![
                (key::encode(&0x0000_0000u32), 0x0000_0000, 0),
                (key::encode(&0x0000_0001u32), 0x0000_0001, 10),
                (key::encode(&0x0000_0002u32), 0x0000_0002, 20),
                (key::encode(&0x0000_0003u32), 0x0000_0003, 30),
                (key::encode(&0x0000_0004u32), 0x0000_0004, 40),
                (key::encode(&0x0000_0005u32), 0x0000_0005, 50),
                (key::encode(&0x0000_0006u32), 0x0000_0006, 60),
                (key::encode(&0x0000_0007u32), 0x0000_0007, 70),
                (key::encode(&0x0000_0008u32), 0x0000_0008, 80),
                (key::encode(&0x0000_0009u32), 0x0000_0009, 90),
            ],
            "Multiple pages, at snapshot 1",
        );

        assert_eq!(paginate(0, 0x0001), vec![], "Empty page, at snapshot 0");

        assert_eq!(
            paginate(1, 0x0001),
            vec![
                (key::encode(&0x0001_0000u32), 0x0001_0000, 1),
                (key::encode(&0x0001_0002u32), 0x0001_0002, 21),
            ],
            "Single page, at snapshot 1"
        );
    }

    #[test]
    fn paginate_prefix_backward() {
        let (_d, map) = map();

        let paginate = |cp: u64, prefix: u16| {
            let mut before: Option<Vec<u8>> = None;
            let mut results = vec![];
            loop {
                let cursor = before.as_deref().unwrap_or_default();
                let resp = Page::from_request(&config(), &[], cursor, 0, End::Back)
                    .paginate_prefix::<_, _, _, Infallible>(&map, cp, &prefix)
                    .unwrap();

                assert_eq!(resp.has_next, before.is_some());
                before = resp.results.first().map(|(c, _, _)| c.clone());
                results.extend(resp.results.into_iter().rev());
                if !resp.has_prev {
                    break;
                }
            }

            results
        };

        assert_eq!(
            paginate(0, 0x0000),
            vec![
                (key::encode(&0x0000_0009u32), 0x0000_0009, 90),
                (key::encode(&0x0000_0007u32), 0x0000_0007, 70),
                (key::encode(&0x0000_0005u32), 0x0000_0005, 50),
                (key::encode(&0x0000_0003u32), 0x0000_0003, 30),
                (key::encode(&0x0000_0001u32), 0x0000_0001, 10),
            ],
            "Multiple pages, at snapshot 0",
        );

        assert_eq!(
            paginate(1, 0x0000),
            vec![
                (key::encode(&0x0000_0009u32), 0x0000_0009, 90),
                (key::encode(&0x0000_0008u32), 0x0000_0008, 80),
                (key::encode(&0x0000_0007u32), 0x0000_0007, 70),
                (key::encode(&0x0000_0006u32), 0x0000_0006, 60),
                (key::encode(&0x0000_0005u32), 0x0000_0005, 50),
                (key::encode(&0x0000_0004u32), 0x0000_0004, 40),
                (key::encode(&0x0000_0003u32), 0x0000_0003, 30),
                (key::encode(&0x0000_0002u32), 0x0000_0002, 20),
                (key::encode(&0x0000_0001u32), 0x0000_0001, 10),
                (key::encode(&0x0000_0000u32), 0x0000_0000, 00),
            ],
            "Multiple pages, at snapshot 1",
        );

        assert_eq!(paginate(0, 0x0001), vec![], "Empty page, at snapshot 0");

        assert_eq!(
            paginate(1, 0x0001),
            vec![
                (key::encode(&0x0001_0002u32), 0x0001_0002, 21),
                (key::encode(&0x0001_0000u32), 0x0001_0000, 1),
            ],
            "Single page, at snapshot 1"
        );
    }

    #[test]
    fn forward_cursor_sandwich() {
        let (_d, map) = map();

        let a = key::encode(&0x0000_0001u32);
        let b = key::encode(&0x0000_0007u32);
        assert_eq!(
            Page::from_request(&config(), &a, &b, 5, End::Front)
                .paginate_prefix::<_, _, _, Infallible>(&map, 0, &0x0000u16)
                .unwrap(),
            Response {
                has_prev: true,
                has_next: true,
                results: vec![
                    (key::encode(&0x0000_0003u32), 0x0000_0003, 30),
                    (key::encode(&0x0000_0005u32), 0x0000_0005, 50),
                ],
            },
            "Cursors point to values in range",
        );

        let a = key::encode(&0x0000_0002u32);
        let b = key::encode(&0x0000_0006u32);
        assert_eq!(
            Page::from_request(&config(), &a, &b, 5, End::Front)
                .paginate_prefix::<_, _, _, Infallible>(&map, 0, &0x0000u16)
                .unwrap(),
            Response {
                has_prev: true,
                has_next: true,
                results: vec![
                    (key::encode(&0x0000_0003u32), 0x0000_0003, 30),
                    (key::encode(&0x0000_0005u32), 0x0000_0005, 50),
                ],
            },
            "Cursors do not point to values in range, but are in range",
        );

        let a = key::encode(&0x0000_0002u32);
        assert_eq!(
            Page::from_request(&config(), &a, &[], 5, End::Front)
                .paginate_prefix::<_, _, _, Infallible>(&map, 1, &0x0001u16)
                .unwrap(),
            Response {
                has_prev: false,
                has_next: false,
                results: vec![
                    (key::encode(&0x0001_0000u32), 0x0001_0000, 1),
                    (key::encode(&0x0001_0002u32), 0x0001_0002, 21),
                ],
            },
            "After cursor is outside of prefix range",
        );

        let a = key::encode(&0x0000_0005u32);
        let b = key::encode(&0x0001_0000u32);
        assert_eq!(
            Page::from_request(&config(), &a, &b, 5, End::Front)
                .paginate_prefix::<_, _, _, Infallible>(&map, 0, &0x0000u16)
                .unwrap(),
            Response {
                has_prev: true,
                has_next: false,
                results: vec![
                    (key::encode(&0x0000_0007u32), 0x0000_0007, 70),
                    (key::encode(&0x0000_0009u32), 0x0000_0009, 90),
                ],
            },
            "Before cursor is outside of prefix range",
        );

        let a = key::encode(&0x0000_0005u32);
        let b = key::encode(&0x0000_0009u32);
        assert_eq!(
            Page::from_request(&config(), &a, &b, 5, End::Front)
                .paginate_prefix::<_, _, _, Infallible>(&map, 1, &0x0001u16)
                .unwrap(),
            Response {
                has_prev: false,
                has_next: true,
                results: vec![],
            },
            "Both cursors before prefix range",
        );

        let a = key::encode(&0x0001_0000u32);
        let b = key::encode(&0x0001_0002u32);
        assert_eq!(
            Page::from_request(&config(), &a, &b, 5, End::Front)
                .paginate_prefix::<_, _, _, Infallible>(&map, 0, &0x0000u16)
                .unwrap(),
            Response {
                has_prev: true,
                has_next: false,
                results: vec![],
            },
            "Both cursors after prefix range",
        );

        let a = key::encode(&0x0001_0000u32);
        let b = key::encode(&0x0001_0004u32);
        assert_eq!(
            Page::from_request(&config(), &a, &b, 5, End::Back)
                .paginate_prefix::<_, _, _, Infallible>(&map, 0, &0x0001u16)
                .unwrap(),
            Response {
                has_prev: false,
                has_next: false,
                results: vec![],
            },
            "Cursors are outside range because range is empty",
        );
    }

    #[test]
    fn backward_cursor_sandwich() {
        let (_d, map) = map();

        let a = key::encode(&0x0000_0001u32);
        let b = key::encode(&0x0000_0007u32);
        assert_eq!(
            Page::from_request(&config(), &a, &b, 5, End::Back)
                .paginate_prefix::<_, _, _, Infallible>(&map, 0, &0x0000u16)
                .unwrap(),
            Response {
                has_prev: true,
                has_next: true,
                results: vec![
                    (key::encode(&0x0000_0003u32), 0x0000_0003, 30),
                    (key::encode(&0x0000_0005u32), 0x0000_0005, 50),
                ],
            },
            "Cursors point to values in range",
        );

        let a = key::encode(&0x0000_0002u32);
        let b = key::encode(&0x0000_0006u32);
        assert_eq!(
            Page::from_request(&config(), &a, &b, 5, End::Back)
                .paginate_prefix::<_, _, _, Infallible>(&map, 0, &0x0000u16)
                .unwrap(),
            Response {
                has_prev: true,
                has_next: true,
                results: vec![
                    (key::encode(&0x0000_0003u32), 0x0000_0003, 30),
                    (key::encode(&0x0000_0005u32), 0x0000_0005, 50),
                ],
            },
            "Cursors do not point to values in range, but are in range",
        );

        let a = key::encode(&0x0000_0002u32);
        assert_eq!(
            Page::from_request(&config(), &a, &[], 5, End::Back)
                .paginate_prefix::<_, _, _, Infallible>(&map, 1, &0x0001u16)
                .unwrap(),
            Response {
                has_prev: false,
                has_next: false,
                results: vec![
                    (key::encode(&0x0001_0000u32), 0x0001_0000, 1),
                    (key::encode(&0x0001_0002u32), 0x0001_0002, 21),
                ],
            },
            "After cursor is outside of prefix range",
        );

        let a = key::encode(&0x0000_0005u32);
        let b = key::encode(&0x0001_0000u32);
        assert_eq!(
            Page::from_request(&config(), &a, &b, 5, End::Back)
                .paginate_prefix::<_, _, _, Infallible>(&map, 0, &0x0000u16)
                .unwrap(),
            Response {
                has_prev: true,
                has_next: false,
                results: vec![
                    (key::encode(&0x0000_0007u32), 0x0000_0007, 70),
                    (key::encode(&0x0000_0009u32), 0x0000_0009, 90),
                ],
            },
            "Before cursor is outside of prefix range",
        );

        let a = key::encode(&0x0000_0005u32);
        let b = key::encode(&0x0000_0009u32);
        assert_eq!(
            Page::from_request(&config(), &a, &b, 5, End::Back)
                .paginate_prefix::<_, _, _, Infallible>(&map, 1, &0x0001u16)
                .unwrap(),
            Response {
                has_prev: false,
                has_next: true,
                results: vec![],
            },
            "Both cursors before prefix range",
        );

        let a = key::encode(&0x0001_0000u32);
        let b = key::encode(&0x0001_0002u32);
        assert_eq!(
            Page::from_request(&config(), &a, &b, 5, End::Back)
                .paginate_prefix::<_, _, _, Infallible>(&map, 0, &0x0000u16)
                .unwrap(),
            Response {
                has_prev: true,
                has_next: false,
                results: vec![],
            },
            "Both cursors after prefix range",
        );

        let a = key::encode(&0x0001_0000u32);
        let b = key::encode(&0x0001_0004u32);
        assert_eq!(
            Page::from_request(&config(), &a, &b, 5, End::Back)
                .paginate_prefix::<_, _, _, Infallible>(&map, 0, &0x0001u16)
                .unwrap(),
            Response {
                has_prev: false,
                has_next: false,
                results: vec![],
            },
            "Cursors are outside range because range is empty",
        );
    }

    #[test]
    fn checkpoint_not_in_range() {
        let (_d, map) = map();

        assert!(matches!(
            Page::from_request(&config(), &[], &[], 0, End::Front)
                .paginate_prefix::<_, _, _, Infallible>(&map, 2, &0x0000u16)
                .expect_err("Should fail with NotInRange error"),
            RpcError::NotInRange(2),
        ));

        assert!(matches!(
            Page::from_request(&config(), &[], &[], 0, End::Back)
                .paginate_prefix::<_, _, _, Infallible>(&map, 2, &0x0000u16)
                .expect_err("Should fail with NotInRange error"),
            RpcError::NotInRange(2),
        ));
    }
}
