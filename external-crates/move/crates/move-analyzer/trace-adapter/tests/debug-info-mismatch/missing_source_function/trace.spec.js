// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

let action = (runtime) => {
    // Models a stale source package where a traced function is absent from
    // source debug info but present in bytecode debug info.
    let res = '';

    // The mismatch is later in the trace, so launch should not warn yet.
    res += warnings_to_string(runtime);
    res += runtime.toString();

    // Enter the function missing from source debug info; warn and show bytecode.
    runtime.step(false);
    res += warnings_to_string(runtime);
    res += runtime.toString();

    // Leave the bytecode-only frame; source debugging resumes in the caller.
    runtime.step(false);
    res += warnings_to_string(runtime);
    res += runtime.toString();
    return res;
};
run_spec(__dirname, action);
