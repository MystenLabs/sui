let action = (runtime) => {
    let res = '';
    // step into a function, which immediately step in to a macro
    runtime.step(false);
    // step inside the macro
    runtime.step(false);
    res += runtime.toString();
    // step into lambda
    runtime.step(false);
    res += runtime.toString();
    // step from lambda back into macro
    runtime.step(false);
    res += runtime.toString();
    return res;
};
run_spec(__dirname, action);
