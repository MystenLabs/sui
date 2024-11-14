const { ExecutionResult } = require('../../out/runtime');

let action = (runtime) => {
    let res = '';
    // continue to reach abort due to incorrect arithmetics
    res += ExecutionResult[runtime.continue()];
    return res;
};
run_spec(__dirname, action);
