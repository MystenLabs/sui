const { ExecutionResultKind } = require('../../out/runtime');

let action = (runtime) => {
    let res = '';
    // continue to the end of the program to get an error
    err = runtime.continue();
    res += ExecutionResultKind[err.kind] + ": " + err.msg;
    return res;
};
run_spec_replay(__dirname, action);
