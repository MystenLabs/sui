const path = require('path');
let action = (runtime) => {
    let res = '';
    // step over a function
    runtime.step(true);
    res += runtime.toString();

    // step into publish
    runtime.step(false);
    res += runtime.toString();

    // step again to leave publish
    runtime.step(false);
    // step into init function
    runtime.step(false);
    res += runtime.toString();

    // step out of a init function
    runtime.stepOut(false);
    // step into transfer
    runtime.step(false);
    res += runtime.toString();

    // step out of transfer
    runtime.stepOut(false);
    // step into make primitive vector
    runtime.step(false);
    res += runtime.toString();


    // step out of make primitive vector
    runtime.stepOut(false);
    // step into "regular" function
    runtime.step(false);
    // another step in the function
    runtime.step(false);
    res += runtime.toString();

    // step out of the function
    runtime.stepOut(false);
    // step into split coins
    runtime.step(false);
    res += runtime.toString();

    // step out of split coins
    runtime.stepOut(false);
    // step into merge coins
    runtime.step(false);
    res += runtime.toString();

    // step out of merge coins
    runtime.stepOut(false);
    // step into make object vector
    runtime.step(false);
    res += runtime.toString();

    return res;
};
run_spec_replay(__dirname, action);
