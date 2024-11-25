let action = (runtime) => {
    let res = '';
    // step over a function containing a native call
    runtime.step(true);
    res += runtime.toString();
    return res;
};
run_spec(__dirname, action);
