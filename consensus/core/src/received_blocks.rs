// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, sync::Arc};

use consensus_types::block::BlockRef;
use parking_lot::Mutex;

/// Tracks blocks that have been received from the network but not yet processed by Core:
/// they are being verified, or waiting in the core thread's queue. Until Core processes them
/// they are invisible to DagState and BlockManager, so a block that arrived first and links to
/// them reports them as missing ancestors and the synchronizer would fetch them again.
///
/// A block is tracked while the guard returned by [`ReceivedBlocks::track`] is alive. The same
/// block can be received more than once concurrently (over its author's stream and via a
/// fetch), so entries are reference counted.
pub(crate) struct ReceivedBlocks {
    inner: Mutex<BTreeMap<BlockRef, usize>>,
}

impl ReceivedBlocks {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(BTreeMap::new()),
        })
    }

    /// Marks the blocks as received until the returned guard is dropped.
    pub(crate) fn track(self: &Arc<Self>, block_refs: Vec<BlockRef>) -> ReceivedBlocksGuard {
        {
            let mut inner = self.inner.lock();
            for block_ref in &block_refs {
                *inner.entry(*block_ref).or_default() += 1;
            }
        }
        ReceivedBlocksGuard {
            tracker: self.clone(),
            block_refs,
        }
    }

    pub(crate) fn contains(&self, block_ref: &BlockRef) -> bool {
        self.inner.lock().contains_key(block_ref)
    }
}

/// Keeps its blocks tracked in [`ReceivedBlocks`] until dropped.
pub(crate) struct ReceivedBlocksGuard {
    tracker: Arc<ReceivedBlocks>,
    block_refs: Vec<BlockRef>,
}

impl Drop for ReceivedBlocksGuard {
    fn drop(&mut self) {
        let mut inner = self.tracker.inner.lock();
        for block_ref in &self.block_refs {
            let count = inner
                .get_mut(block_ref)
                .expect("tracked block must have an entry");
            *count -= 1;
            if *count == 0 {
                inner.remove(block_ref);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use consensus_config::AuthorityIndex;
    use consensus_types::block::BlockDigest;

    use super::*;

    fn block_ref(round: u32) -> BlockRef {
        BlockRef::new(round, AuthorityIndex::new_for_test(0), BlockDigest::MIN)
    }

    #[test]
    fn tracked_until_every_guard_is_dropped() {
        let received = ReceivedBlocks::new();
        assert!(!received.contains(&block_ref(1)));

        let stream_guard = received.track(vec![block_ref(1), block_ref(2)]);
        let fetch_guard = received.track(vec![block_ref(1)]);
        assert!(received.contains(&block_ref(1)));
        assert!(received.contains(&block_ref(2)));

        drop(stream_guard);
        assert!(received.contains(&block_ref(1)));
        assert!(!received.contains(&block_ref(2)));

        drop(fetch_guard);
        assert!(!received.contains(&block_ref(1)));
        assert!(received.inner.lock().is_empty());
    }
}
