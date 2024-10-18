let action = (runtime) => {
    let res = '';
    // step over functions creating data to be referenced
    runtime.step(true);
    runtime.step(true);
    runtime.step(true);
    runtime.step(true);
    // step into a function
    runtime.step(false);
    res += runtime.toString();
    // advance until all references are updated
    runtime.step(true);
    runtime.step(true);
    runtime.step(true);
    runtime.step(true);
    runtime.step(true);
    runtime.step(true);
    res += runtime.toString();
    return res;
};
run_spec(__dirname, action);
