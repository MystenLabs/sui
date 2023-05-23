// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use arc_swap::{ArcSwap, Guard};
use futures::future::{select, Either};
use futures::FutureExt;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use sui_config::Config;
#[cfg(unix)]
use tokio::signal::unix::{Signal, SignalKind};
#[cfg(unix)]
use tracing::info;
use tracing::warn;

#[derive(Clone)]
pub struct LiveConfig<T> {
    current: Arc<ArcSwap<T>>,
    // Dropping all instances of LiveConfig will stop monitoring task
    _exit: tokio::sync::mpsc::Sender<()>,
}

struct ConfigRefresher<T> {
    current: Arc<ArcSwap<T>>,
    path: PathBuf,
    exit: tokio::sync::mpsc::Receiver<()>,
}

impl<T: Config + Send + Sync + 'static> LiveConfig<T> {
    pub fn load_and_auto_refresh<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let path = path.as_ref().to_path_buf();
        let current = T::load(&path)?;
        let current = Arc::new(ArcSwap::from_pointee(current));
        let (exit_sender, exit_receiver) = tokio::sync::mpsc::channel(1);
        let refresher = ConfigRefresher {
            current: current.clone(),
            path,
            exit: exit_receiver,
        };
        #[cfg(unix)]
        let signal = {
            let sig_hup = tokio::signal::unix::signal(SignalKind::hangup()).unwrap();
            SigHupRefreshSignal { sig_hup }
        };
        #[cfg(not(unix))]
        let signal = PeriodicRefreshSignal;
        tokio::spawn(refresher.run(signal));
        Ok(Self {
            current,
            _exit: exit_sender,
        })
    }

    pub fn latest(&self) -> Guard<Arc<T>> {
        self.current.load()
    }
}

trait RefreshSignal {
    fn recv(&mut self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>;
}

#[cfg(unix)]
struct SigHupRefreshSignal {
    sig_hup: Signal,
}

#[cfg(unix)]
impl RefreshSignal for SigHupRefreshSignal {
    fn recv(&mut self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        self.sig_hup.recv().map(|_| ()).boxed()
    }
}

#[cfg(not(unix))]
struct PeriodicRefreshSignal;

#[cfg(not(unix))]
impl RefreshSignal for PeriodicRefreshSignal {
    fn recv(&mut self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        tokio::time::sleep(std::time::Duration::from_secs(30)).boxed()
    }
}

impl<T: Config + Send> ConfigRefresher<T> {
    async fn run<S: RefreshSignal>(mut self, mut signal: S) {
        loop {
            let s = select(signal.recv(), self.exit.recv().boxed()).await;
            match s {
                Either::Left(_hup) => {}
                Either::Right(_exit) => return,
            }
            match T::load(&self.path) {
                Ok(new) => {
                    #[cfg(unix)]
                    info!("Refreshed config at {:?}", self.path);
                    let new = Arc::new(new);
                    self.current.swap(new);
                }
                Err(err) => warn!("Failed to reload config at {:?}: {:?}", self.path, err),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use std::fs::File;
    use std::io::prelude::*;
    use std::time::Duration;
    use tempfile::tempdir;

    #[derive(Serialize, Deserialize)]
    struct TestConfig {
        value: u32,
    }

    impl Config for TestConfig {}

    #[tokio::test]
    async fn live_config_test() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config");
        let mut file = File::create(&path).unwrap();
        file.write_all(b"value: 15").unwrap();
        drop(file);
        let live_config = LiveConfig::<TestConfig>::load_and_auto_refresh(&path).unwrap();
        assert_eq!(15, live_config.latest().value);
        let mut file = File::create(&path).unwrap();
        file.write_all(b"value: 16").unwrap();
        drop(file);
        unsafe {
            let pid = libc::getpid();
            assert!(pid > 0);
            libc::kill(pid, libc::SIGHUP);
        }
        for _ in 0..100 {
            tokio::time::sleep(Duration::from_millis(10)).await;
            let value = live_config.latest().value;
            if value == 15 {
                // wait more
            } else if value == 16 {
                return; // pass
            } else {
                panic!("Got unexpected value {}", value)
            }
        }
        panic!("Did not get value after waiting for a long time");
    }
}
