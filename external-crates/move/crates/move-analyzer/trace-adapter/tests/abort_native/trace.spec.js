const { run } = require('node:test');
const { ExecutionResult } = require('../../out/runtime');

let action = (runtime) => {
    let res = '';
    // step into function creating a vector
    runtime.step(false);
    // step out of a function creating a vector
    runtime.stepOut();
    // step into function containing native call
    runtime.step(false);
    // step out of a function containing native call
    // before this call is executed
    res += ExecutionResult[runtime.stepOut()];
    return res;
};
run_spec(__dirname, action);
