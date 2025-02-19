const path = require('path');
let action = (runtime) => {
    let res = '';
    res += runtime.toString();
    // step into a function
    runtime.step(false);
    runtime.step(false);
    res += runtime.toString();
    return res;
};
run_spec_replay(__dirname, action);
