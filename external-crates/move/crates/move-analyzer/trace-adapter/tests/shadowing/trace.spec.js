let action = (runtime) => {
    let res = '';
    // step into a function
    runtime.step(false);
    // advance until first shadowed variable is created
    runtime.step(true);
    runtime.step(true);
    runtime.step(true);
    res += runtime.toString();
    // advance until second shadowed variable is created
    runtime.step(true);
    runtime.step(true);
    res += runtime.toString();
    // advance until second shadowed variable disappears
    runtime.step(true);
    runtime.step(true);
    runtime.step(true);
    res += runtime.toString();
    // advance until first shadowed variable disappears
    runtime.step(true);
    res += runtime.toString();

    return res;
};
run_spec(__dirname, action);
