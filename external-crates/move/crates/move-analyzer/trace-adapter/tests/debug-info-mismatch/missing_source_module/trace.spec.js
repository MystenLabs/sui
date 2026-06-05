// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

let action = (runtime) => {
    // Models a source-mapped caller entering a module absent from source debug info.
    let res = '';

    res += warnings_to_string(runtime);
    res += runtime.toString();

    runtime.step(false);
    res += warnings_to_string(runtime);
    res += runtime.toString();

    // Return from the bytecode-only module to source-level debugging in the caller.
    runtime.step(false);
    res += warnings_to_string(runtime);
    res += runtime.toString();
    return res;
};
run_spec(__dirname, action);
