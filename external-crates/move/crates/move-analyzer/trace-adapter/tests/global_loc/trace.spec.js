let action = (runtime) => {
    let res = '';
    // step over context creation
    runtime.step(true);
    // step over function creating a global location
    runtime.step(true);
    res += runtime.toString();
    return res;
};
run_spec(__dirname, action);
