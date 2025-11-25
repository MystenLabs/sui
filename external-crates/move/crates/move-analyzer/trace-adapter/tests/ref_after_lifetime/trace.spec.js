let action = (runtime) => {
    // Step into return_ref_param
    runtime.step(false); // Step 1
    runtime.step(false); // Step 2 - inside return_ref_param now

    // toString() displays the stack and variables
    // The reference parameter points to a dead local, triggering the bug
    return runtime.toString();
};
run_spec(__dirname, action);
