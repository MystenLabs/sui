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
    // we need to step into lambda and back to finish the macro,
    // likely because how lambda is compiled not being a real function
    runtime.step(false);
    runtime.step(false);
    // step into the caller function and take the next step to show
    // result of macro execution
    runtime.step(false);
    runtime.step(false);
    res += runtime.toString();
    return res;
};
run_spec(__dirname, action);
