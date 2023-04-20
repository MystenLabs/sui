// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use named_lock::{NamedLock, NamedLockGuard};
use once_cell::sync::Lazy;
use sha2::{Digest, Sha256};
use std::sync::{Mutex, MutexGuard};
use whoami::username;

const PACKAGE_LOCK_NAME: &str = "move_pkg_lock";
static PACKAGE_THREAD_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));
static PACKAGE_PROCESS_MUTEX: Lazy<NamedLock> = Lazy::new(|| lock_username(username()));

fn lock_username(unsanitized_username: String) -> NamedLock {
    let hashed_username = format!("{:X}", Sha256::digest(unsanitized_username.as_bytes()));
    let mut user_lock_file = format!("{}_{}", PACKAGE_LOCK_NAME, hashed_username);
    // truncate to 255 characters to avoid filname length limit
    user_lock_file.truncate(255);
    NamedLock::create(user_lock_file.as_str()).unwrap()
}

/// The package lock is a lock held across threads and processes. This lock is held to ensure that
/// the Move package manager has a consistent (read: serial) view of the file system. Without this
/// lock we can easily get into race conditions around caching and overwriting of packages (e.g.,
/// thread 1 and thread 2 compete to build package P in the same location), as well as downloading
/// of git dependencies (thread 1 starts downloading git dependency, meanwhile thread 2 sees the
/// git directory before it has been fully populated but assumes it has been fully downloaded and
/// starts building the package before the git dependency has been fully downloaded by thread 1.
/// This will then lead to file not found errors). These same issues could occur across processes,
/// this is why we grab both a thread lock and process lock.
pub(crate) struct PackageLock {
    _thread_lock: MutexGuard<'static, ()>,
    _process_lock: NamedLockGuard<'static>,
}

impl PackageLock {
    pub(crate) fn lock() -> PackageLock {
        Self {
            _thread_lock: PACKAGE_THREAD_MUTEX.lock().unwrap(),
            _process_lock: PACKAGE_PROCESS_MUTEX.lock().unwrap(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn package_lock_slashes_in_username() {
        lock_username("user/name".to_string());
    }

    #[test]
    fn package_lock_unicode() {
        lock_username("🦀".to_string());
    }

    #[test]
    fn package_lock_unicode_bad() {
        lock_username("𱍐".to_string());
    }

    proptest! {
        #[test]
        fn package_lock_username_does_not_crash(username in "\\PC*") {
            lock_username(username);
        }
    }
}
