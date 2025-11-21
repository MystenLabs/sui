const { ExecutionResultKind } = require('../../out/runtime');

let action = (runtime) => {
    let res = '';
    // step over a function to get abort state
    err = runtime.step(true);
    res += ExecutionResultKind[err.kind] + ": " + err.msg;
    return res;
};
run_spec(__dirname, action);
