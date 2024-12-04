const { ExecutionResult } = require('../../out/runtime');

let action = (runtime) => {
    let res = '';
    // step over a function to get abort state
    res += ExecutionResult[runtime.step(true)];
    return res;
};
run_spec(__dirname, action);
