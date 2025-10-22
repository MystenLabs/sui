const { ExecutionResultKind } = require('../../out/runtime');

let action = (runtime) => {
    let res = '';
    // continue to reach abort due to incorrect arithmetics
    err = runtime.continue();
    res += ExecutionResultKind[err.kind] + ": " + err.msg;
    return res;
};
run_spec(__dirname, action);
