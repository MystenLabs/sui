const { ExecutionResult } = require('../../out/runtime');

let action = (runtime) => {
    let res = '';
    // continue to the end of the program to get abort state
    res += ExecutionResult[runtime.continue()];
    return res;
};
run_spec(__dirname, action);
