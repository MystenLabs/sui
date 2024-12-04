const { run } = require('node:test');
const path = require('path');
let action = (runtime) => {
    const filePath = path.join(__dirname, 'sources', `m_dep.move`);
    let res = '';
    // step into a function, which immediately step in to a macro
    runtime.step(false);
    res += runtime.toString();
    // step to the second macro
    runtime.step(false);
    res += runtime.toString();
    // step to leave the second macro, and keep stepping
    // to leave to the outer function
    runtime.step(false);
    runtime.step(false);
    runtime.step(false);
    runtime.step(false);
    runtime.step(false);
    runtime.step(false);
    runtime.step(false);
    runtime.step(false);
    res += runtime.toString();
    // set a breakpoint in the inner macro and continue to hit it
    runtime.setLineBreakpoints(filePath, [4]);
    runtime.continue();
    res += runtime.toString();
    return res;
};
run_spec(__dirname, action);
