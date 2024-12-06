const { ExecutionResult } = require('../../out/runtime');

let action = (runtime) => {
    let res = '';
    // keep stepping to get abort state
    runtime.step(false);
    runtime.step(false);
    res += ExecutionResult[runtime.step(false)];
    return res;
};
run_spec(__dirname, action);
