const path = require('path');
let action = (runtime) => {
    let res = '';
    // we are in a functino that has source file
    res += runtime.toString();
    // step into a function which does not have source file
    runtime.step(false);
    res += runtime.toString();
    // step until you enter function that has source file
    runtime.step(false);
    runtime.step(false);
    runtime.step(false);
    runtime.step(false);
    res += runtime.toString();
    return res;
};
run_spec(__dirname, action);
