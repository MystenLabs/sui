let action = (runtime) => {
    let res = '';
    // step into the main test function
    runtime.step(false);
    res += runtime.toString();

    // step over a function to the next line
    runtime.step(true);
    res += runtime.toString();

    // step over two functions to the next line
    runtime.step(true);
    res += runtime.toString();

    // step into a function
    runtime.step(false);
    // step out of the function to the same line
    runtime.stepOut(false);
    res += runtime.toString();
    // step into a function
    runtime.step(false);
    // step out of the function to the next line
    runtime.stepOut(false);
    res += runtime.toString();
    return res;
};
run_spec(__dirname, action);
