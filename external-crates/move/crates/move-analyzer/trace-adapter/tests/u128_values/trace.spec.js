// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

let action = (runtime) => {
    // Step into `observe`, where all three u128 parameters are bound, and
    // snapshot their reconstructed values.
    runtime.step(false);
    runtime.step(false);
    runtime.step(false);
    runtime.step(false);
    return runtime.toString();
};
run_spec(__dirname, action);
