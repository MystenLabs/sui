// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! A monotone lower bound on the latest version of objects, used to accelerate
//! post-consensus owned-object conflict detection.
//!
//! The consensus commit handler decides conflicts with the rule
//! `conflict ⟺ latest_version(id) > claimed_version` (see
//! `docs/objects_locking.md` Part 3). Reading the objects table for that on the
//! CPU-bound handler thread is expensive, so this cache memoizes *observations*
//! of latest-object state made elsewhere — primarily by vote-time validation in
//! `SuiTxValidator`, which reads every owned input of every transaction it
//! votes on, shortly before the same refs reach the commit handler.
//!
//! Correctness rests on observations being lower bounds: versions never
//! decrease, and objects never disappear once they (or their tombstones)
//! exist, so any previously observed state is a valid lower bound on the
//! current state. This means the cache needs no invalidation, arbitrary
//! eviction is safe, and a stale entry can only cause a fallback read, never a
//! wrong verdict: conflict resolution treats a bound as decisive only in the
//! consumed direction (bound above the claimed version), and everything else
//! falls back to an authoritative read of latest-object state (see
//! `docs/objects_locking.md` §3.5/§3.6a).
//!
//! Entries are process-lifetime and epoch-agnostic (version bounds stay valid
//! across epoch boundaries and are naturally empty after a restart).

use moka::sync::SegmentedCache as MokaCache;
use sui_types::base_types::{ObjectID, SequenceNumber};
use sui_types::object::Object;

/// A lower bound on the latest version of an object, from an authoritative
/// read of latest-object state (live object cache / objects table).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VersionLowerBound {
    /// The object id had no version and no tombstone at observation time.
    KnownAbsent,
    /// The latest version was at least `version` at observation time.
    Version {
        version: SequenceNumber,
        /// Whether the object was immutable at exactly `version`. Ownership of
        /// a given version never changes, so this bit is stable per version.
        /// Tombstones record `false`.
        immutable: bool,
    },
}

impl VersionLowerBound {
    pub fn from_object(object: &Object) -> Self {
        VersionLowerBound::Version {
            version: object.version(),
            immutable: object.is_immutable(),
        }
    }

    fn version_key(&self) -> Option<SequenceNumber> {
        match self {
            VersionLowerBound::KnownAbsent => None,
            VersionLowerBound::Version { version, .. } => Some(*version),
        }
    }

    /// The greater (more informative) of two bounds. `KnownAbsent` is the
    /// bottom element. Equal versions carry equal `immutable` bits, so which
    /// one wins is immaterial.
    fn merge(self, other: Self) -> Self {
        if other.version_key() > self.version_key() {
            other
        } else {
            self
        }
    }
}

pub struct LiveObjectCache {
    entries: MokaCache<ObjectID, VersionLowerBound>,
}

const DEFAULT_CAPACITY: u64 = 1_000_000;

impl LiveObjectCache {
    pub fn new() -> Self {
        let capacity = std::env::var("LIVE_OBJECT_CACHE_SIZE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_CAPACITY);
        Self::with_capacity(capacity)
    }

    pub fn with_capacity(capacity: u64) -> Self {
        Self {
            entries: MokaCache::builder(8).max_capacity(capacity).build(),
        }
    }

    /// Record an observed bound, keeping the max of the existing and new
    /// bounds. (A plain overwrite would also be correct — every observation is
    /// a valid lower bound — but max-merge avoids a slow observer regressing a
    /// fresher entry into extra fallback reads.)
    pub fn record(&self, id: ObjectID, bound: VersionLowerBound) {
        self.entries.entry(id).and_upsert_with(|existing| {
            existing.map_or(bound, |entry| entry.into_value().merge(bound))
        });
    }

    pub fn record_object(&self, object: &Object) {
        self.record(object.id(), VersionLowerBound::from_object(object));
    }

    /// Record that an id had no version and no tombstone. Only call this from
    /// reads that distinguish tombstones from true absence
    /// (`get_latest_object_ref_or_tombstone`-style); a live-object read that
    /// returns `None` for a deleted object must not be recorded as absent.
    pub fn record_absent(&self, id: ObjectID) {
        self.record(id, VersionLowerBound::KnownAbsent);
    }

    pub fn get(&self, id: &ObjectID) -> Option<VersionLowerBound> {
        self.entries.get(id)
    }
}

impl Default for LiveObjectCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(v: u64) -> VersionLowerBound {
        VersionLowerBound::Version {
            version: SequenceNumber::from_u64(v),
            immutable: false,
        }
    }

    #[test]
    fn test_monotone_merge() {
        let cache = LiveObjectCache::with_capacity(100);
        let id = ObjectID::random();

        assert_eq!(cache.get(&id), None);

        cache.record_absent(id);
        assert_eq!(cache.get(&id), Some(VersionLowerBound::KnownAbsent));

        cache.record(id, version(5));
        assert_eq!(cache.get(&id), Some(version(5)));

        // Older observations never regress the bound.
        cache.record(id, version(3));
        assert_eq!(cache.get(&id), Some(version(5)));
        cache.record_absent(id);
        assert_eq!(cache.get(&id), Some(version(5)));

        cache.record(id, version(7));
        assert_eq!(cache.get(&id), Some(version(7)));
    }

    #[test]
    fn test_immutable_bit_follows_version() {
        let cache = LiveObjectCache::with_capacity(100);
        let id = ObjectID::random();

        cache.record(
            id,
            VersionLowerBound::Version {
                version: SequenceNumber::from_u64(2),
                immutable: true,
            },
        );
        cache.record(id, version(1));
        assert_eq!(
            cache.get(&id),
            Some(VersionLowerBound::Version {
                version: SequenceNumber::from_u64(2),
                immutable: true,
            })
        );
    }
}
