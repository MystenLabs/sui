const { ExecutionResultKind } = require('../../out/runtime');

let action = (runtime) => {
    let res = '';
    // keep stepping to get abort state
    runtime.step(false);
    runtime.step(false);
    err = runtime.step(false);
    res += ExecutionResultKind[err.kind] + ": " + err.msg;
    return res;
};
run_spec(__dirname, action);
