const { run } = require('node:test');
const { ExecutionResultKind } = require('../../out/runtime');

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
    err = runtime.stepOut();
    res += ExecutionResultKind[err.kind] + ": " + err.msg;
    return res;
};
run_spec(__dirname, action);
