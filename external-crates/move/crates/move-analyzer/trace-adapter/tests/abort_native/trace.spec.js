const { ExecutionResult } = require('../../out/runtime');

let action = (runtime) => {
    let res = '';
    // step into function containing native vall
    runtime.step(false);
    // step out of a function containing native call
    // before this call is executed
    res += ExecutionResult[runtime.stepOut()];
    return res;
};
run_spec(__dirname, action);
