const { ExecutionResultKind } = require('../../out/runtime');

let action = (runtime) => {
    let res = '';
    // continue to the end of the program to get abort state
    err = runtime.continue();
    res += ExecutionResultKind[err.kind] + ": " + err.msg;
    return res;
};
run_spec(__dirname, action);
