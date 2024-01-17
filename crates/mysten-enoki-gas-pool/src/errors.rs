// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[macro_export]
macro_rules! retry_with_max_delay {
    ($func:expr, $max_delay:expr) => {{
        let retry_strategy = ExponentialBackoff::from_millis(50).max_delay($max_delay);
        Retry::spawn(retry_strategy, || $func).await
    }};
}

#[cfg(not(test))]
#[macro_export]
macro_rules! retry_forever {
    ($func:expr) => {{
        let retry_strategy = FixedInterval::from_millis(500);
        Retry::spawn(retry_strategy, || $func).await
    }};
}

#[cfg(test)]
#[macro_export]
macro_rules! retry_forever {
    ($func:expr) => {{
        $func.await
    }};
}
