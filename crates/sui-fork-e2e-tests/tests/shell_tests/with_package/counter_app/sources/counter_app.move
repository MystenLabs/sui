// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Exercise cross-package calls through a published dependency.
module counter_app::counter_app;

use counter::counter::Counter;

/// Increment `counter` twice through the package dependency.
public fun increment_twice(counter: &mut Counter) {
    counter.increment();
    counter.increment();
}
