// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::DRAINING;
use std::sync::atomic::Ordering;
use tracing::{info, warn};

/// Install a SIGUSR1 handler that sets the DRAINING flag to true.
/// This is used for graceful shutdown in Kubernetes environments.
pub fn install_drain_signal_handler() {
    // Check environment variable (defaults to enabled)
    let enable = std::env::var("SUI_HEALTH_ENABLE_SIGUSR1")
        .map(|v| v == "true")
        .unwrap_or(true);
    
    if !enable {
        info!("SIGUSR1 drain signal handler disabled by SUI_HEALTH_ENABLE_SIGUSR1");
        return;
    }

    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        
        tokio::spawn(async {
            let mut sigusr1 = match signal(SignalKind::user_defined1()) {
                Ok(signal) => signal,
                Err(e) => {
                    warn!("Failed to install SIGUSR1 handler: {}", e);
                    return;
                }
            };
            
            loop {
                sigusr1.recv().await;
                info!("Received SIGUSR1, setting DRAINING flag");
                DRAINING.store(true, Ordering::Relaxed);
            }
        });
        
        info!("SIGUSR1 drain signal handler installed");
    }
    
    #[cfg(not(unix))]
    {
        warn!("SIGUSR1 drain signal handler not supported on non-Unix platforms");
    }
}