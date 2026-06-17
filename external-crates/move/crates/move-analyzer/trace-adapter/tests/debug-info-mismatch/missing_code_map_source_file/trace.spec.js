// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

let action = (runtime) => {
    // Models a source-mapped caller entering a function whose source code map
    // points at a file hash missing from the user's supplied sources.
    let res = '';

    // The mismatch is in the callee, so launch should not warn yet.
    res += snapshot(runtime);

    // Enter the function with an unusable source map; warn and show bytecode.
    runtime.step(false);
    res += snapshot(runtime);

    // Leave the bytecode-only frame; source debugging resumes in the caller.
    runtime.step(false);
    res += snapshot(runtime);
    return res;
};
run_spec(__dirname, action);
