let action = (runtime) => {
    let res = '';
    // step over a function creating a complex struct
    runtime.step(true);
    // step into a function
    runtime.step(false);
    res += runtime.toString();
    // advance until all struct fields are updated
    runtime.step(true);
    runtime.step(true);
    runtime.step(true);
    runtime.step(true);
    runtime.step(true);
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
