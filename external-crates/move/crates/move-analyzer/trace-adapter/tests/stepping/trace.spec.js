let action = (runtime) => {
    let res = '';
    // step into a function
    runtime.step(false);
    res += runtime.toString();
    // step out of a function
    runtime.stepOut(false);
    res += runtime.toString();
    // step over a function
    runtime.step(true);
    res += runtime.toString();
    return res;
};
run_spec(__dirname, action);
