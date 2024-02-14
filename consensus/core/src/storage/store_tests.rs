// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use consensus_config::AuthorityIndex;
use rstest::rstest;
use tempfile::TempDir;

use super::{mem_store::MemStore, rocksdb_store::RocksDBStore, Store};
use crate::{
    block::{BlockDigest, BlockRef, TestBlock, VerifiedBlock},
    commit::Commit,
};

/// Test fixture for store tests. Wraps around various store implementations.
enum TestStore {
    RocksDB((RocksDBStore, TempDir)),
    Mem(MemStore),
}

impl TestStore {
    fn new_rocksdb_store() -> Self {
        let temp_dir = TempDir::new().unwrap();
        TestStore::RocksDB((
            RocksDBStore::new(temp_dir.path().to_str().unwrap()),
            temp_dir,
        ))
    }

    fn new_mem_store() -> Self {
        TestStore::Mem(MemStore::new())
    }

    fn store(&self) -> &dyn Store {
        match self {
            TestStore::RocksDB((store, _)) => store,
            TestStore::Mem(store) => store,
        }
    }
}

#[rstest]
#[tokio::test]
async fn read_and_contain_blocks(
    #[values(TestStore::new_rocksdb_store(), TestStore::new_mem_store())] test_store: TestStore,
) {
    let store = test_store.store();

    let written_blocks: Vec<VerifiedBlock> = vec![
        VerifiedBlock::new_for_test(TestBlock::new(1, 1).build()),
        VerifiedBlock::new_for_test(TestBlock::new(1, 0).build()),
        VerifiedBlock::new_for_test(TestBlock::new(1, 2).build()),
        VerifiedBlock::new_for_test(TestBlock::new(2, 3).build()),
    ];
    store.write(written_blocks.clone(), vec![]).unwrap();

    {
        let refs = vec![written_blocks[0].reference()];
        let read_blocks = store
            .read_blocks(&refs)
            .expect("Read blocks should not fail");
        assert_eq!(read_blocks.len(), 1);
        assert_eq!(read_blocks[0].as_ref().unwrap(), &written_blocks[0]);
    }

    {
        let refs = vec![
            written_blocks[2].reference(),
            written_blocks[1].reference(),
            written_blocks[1].reference(),
        ];
        let read_blocks = store
            .read_blocks(&refs)
            .expect("Read blocks should not fail");
        assert_eq!(read_blocks.len(), 3);
        assert_eq!(read_blocks[0].as_ref().unwrap(), &written_blocks[2]);
        assert_eq!(read_blocks[1].as_ref().unwrap(), &written_blocks[1]);
        assert_eq!(read_blocks[2].as_ref().unwrap(), &written_blocks[1]);
    }

    {
        let refs = vec![
            written_blocks[3].reference(),
            BlockRef::new(1, AuthorityIndex::new_for_test(3), BlockDigest::default()),
            written_blocks[2].reference(),
        ];
        let read_blocks = store
            .read_blocks(&refs)
            .expect("Read blocks should not fail");
        assert_eq!(read_blocks.len(), 3);
        assert_eq!(read_blocks[0].as_ref().unwrap(), &written_blocks[3]);
        assert!(read_blocks[1].is_none());
        assert_eq!(read_blocks[2].as_ref().unwrap(), &written_blocks[2]);

        let contain_blocks = store
            .contains_blocks(&refs)
            .expect("Contain blocks should not fail");
        assert_eq!(contain_blocks.len(), 3);
        assert!(contain_blocks[0]);
        assert!(!contain_blocks[1]);
        assert!(contain_blocks[2]);
    }
}

#[rstest]
#[tokio::test]
async fn scan_blocks(
    #[values(TestStore::new_rocksdb_store(), TestStore::new_mem_store())] test_store: TestStore,
) {
    let store = test_store.store();

    let written_blocks = vec![
        VerifiedBlock::new_for_test(TestBlock::new(9, 0).build()),
        VerifiedBlock::new_for_test(TestBlock::new(10, 0).build()),
        VerifiedBlock::new_for_test(TestBlock::new(10, 1).build()),
        VerifiedBlock::new_for_test(TestBlock::new(11, 1).build()),
        VerifiedBlock::new_for_test(TestBlock::new(11, 3).build()),
        VerifiedBlock::new_for_test(TestBlock::new(12, 1).build()),
        VerifiedBlock::new_for_test(TestBlock::new(13, 2).build()),
        VerifiedBlock::new_for_test(TestBlock::new(13, 1).build()),
    ];
    store.write(written_blocks.clone(), vec![]).unwrap();

    {
        let scanned_blocks = store
            .scan_blocks_by_author(AuthorityIndex::new_for_test(1), 20)
            .expect("Scan blocks should not fail");
        assert!(scanned_blocks.is_empty(), "{:?}", scanned_blocks);
    }

    {
        let scanned_blocks = store
            .scan_blocks_by_author(AuthorityIndex::new_for_test(1), 12)
            .expect("Scan blocks should not fail");
        assert_eq!(scanned_blocks.len(), 2, "{:?}", scanned_blocks);
        assert_eq!(
            scanned_blocks,
            vec![written_blocks[5].clone(), written_blocks[7].clone()]
        );
    }

    let additional_blocks = vec![
        VerifiedBlock::new_for_test(TestBlock::new(14, 2).build()),
        VerifiedBlock::new_for_test(TestBlock::new(15, 0).build()),
        VerifiedBlock::new_for_test(TestBlock::new(15, 1).build()),
        VerifiedBlock::new_for_test(TestBlock::new(16, 3).build()),
    ];
    store.write(additional_blocks.clone(), vec![]).unwrap();

    {
        let scanned_blocks = store
            .scan_blocks_by_author(AuthorityIndex::new_for_test(1), 10)
            .expect("Scan blocks should not fail");
        assert_eq!(scanned_blocks.len(), 5, "{:?}", scanned_blocks);
        assert_eq!(
            scanned_blocks,
            vec![
                written_blocks[2].clone(),
                written_blocks[3].clone(),
                written_blocks[5].clone(),
                written_blocks[7].clone(),
                additional_blocks[2].clone(),
            ]
        );
    }

    {
        let scanned_blocks = store
            .scan_last_blocks_by_author(AuthorityIndex::new_for_test(1), 2)
            .expect("Scan blocks should not fail");
        assert_eq!(scanned_blocks.len(), 2, "{:?}", scanned_blocks);
        assert_eq!(
            scanned_blocks,
            vec![written_blocks[7].clone(), additional_blocks[2].clone()]
        );

        let scanned_blocks = store
            .scan_last_blocks_by_author(AuthorityIndex::new_for_test(1), 0)
            .expect("Scan blocks should not fail");
        assert_eq!(scanned_blocks.len(), 0);
    }
}

#[rstest]
#[tokio::test]
async fn read_and_scan_commits(
    #[values(TestStore::new_rocksdb_store(), TestStore::new_mem_store())] test_store: TestStore,
) {
    let store = test_store.store();

    {
        let last_commit = store
            .read_last_commit()
            .expect("Read last commit should not fail");
        assert!(last_commit.is_none(), "{:?}", last_commit);
    }

    let written_commits = vec![
        Commit {
            index: 1,
            leader: BlockRef::new(1, AuthorityIndex::new_for_test(0), BlockDigest::default()),
            ..Default::default()
        },
        Commit {
            index: 2,
            leader: BlockRef::new(2, AuthorityIndex::new_for_test(0), BlockDigest::default()),
            ..Default::default()
        },
        Commit {
            index: 3,
            leader: BlockRef::new(3, AuthorityIndex::new_for_test(0), BlockDigest::default()),
            ..Default::default()
        },
        Commit {
            index: 4,
            leader: BlockRef::new(4, AuthorityIndex::new_for_test(0), BlockDigest::default()),
            ..Default::default()
        },
    ];
    store.write(vec![], written_commits.clone()).unwrap();

    {
        let last_commit = store
            .read_last_commit()
            .expect("Read last commit should not fail");
        assert_eq!(
            last_commit.as_ref(),
            written_commits.last(),
            "{:?}",
            last_commit
        );
    }

    {
        let scanned_commits = store
            .scan_commits(20)
            .expect("Scan commits should not fail");
        assert!(scanned_commits.is_empty(), "{:?}", scanned_commits);
    }

    {
        let scanned_commits = store.scan_commits(3).expect("Scan commits should not fail");
        assert_eq!(scanned_commits.len(), 2, "{:?}", scanned_commits);
        assert_eq!(
            scanned_commits,
            vec![written_commits[2].clone(), written_commits[3].clone()]
        );
    }

    {
        let scanned_commits = store.scan_commits(0).expect("Scan commits should not fail");
        assert_eq!(scanned_commits.len(), 4, "{:?}", scanned_commits);
        assert_eq!(scanned_commits, written_commits,);
    }
}
