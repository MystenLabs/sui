// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

let action = (runtime) => {
    // The top-level function stays in source, while a callee with missing
    // inlined source-map debug info is forced to disassembly. Toggling back to
    // source must not re-enable source for that callee, and its callees with
    // valid inlined source maps must remain source-visible.
    let res = '';

    // Launch in the source-mapped caller with no warning.
    res += snapshot(runtime);

    // Enter the callee with missing inlined source-map debug info; warn and show bytecode.
    runtime.step(false);
    res += snapshot(runtime);

    // Step inside the forced-disassembly callee before it calls a source-visible child.
    runtime.step(false);
    res += snapshot(runtime);

    // Enter a child of the forced-disassembly frame; it still uses source lines.
    runtime.step(false);
    res += snapshot(runtime);

    // Step into the child's valid macro expansion.
    runtime.step(false);
    res += snapshot(runtime);

    // Toggling source while the forced parent is active does not leave bytecode view.
    runtime.setCurrentMoveFileFromFrame(1);
    runtime.toggleSource();
    res += snapshot(runtime);

    // Leave the valid macro expansion and stay in source in the child frame.
    runtime.step(false);
    res += snapshot(runtime);

    // Step to the missing inlined source-map instruction; the forced parent stays in bytecode.
    runtime.step(false);
    res += snapshot(runtime);

    // Step past the missing inline instruction to the next call; the forced parent remains in bytecode.
    runtime.step(false);
    res += snapshot(runtime);

    // Enter another child of the forced-disassembly frame; source view still works.
    runtime.step(false);
    res += snapshot(runtime);

    // Step into the second child's valid macro expansion.
    runtime.step(false);
    res += snapshot(runtime);

    // Leave the second valid macro expansion and stay in source in the child frame.
    runtime.step(false);
    res += snapshot(runtime);

    // Return to the forced parent in bytecode.
    runtime.step(false);
    res += snapshot(runtime);

    // Leave the forced parent; source debugging resumes in the caller.
    runtime.step(false);
    res += snapshot(runtime);
    return res;
};
run_spec(__dirname, action);
