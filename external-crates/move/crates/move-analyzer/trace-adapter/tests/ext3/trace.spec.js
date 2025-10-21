const { ExecutionResult } = require('../../out/runtime');

let action = (runtime) => {
    let res = '';
    // continue to the end of the program to get an error
    res += ExecutionResult[runtime.continue()];
    return res;
};
run_spec_replay(__dirname, action);
