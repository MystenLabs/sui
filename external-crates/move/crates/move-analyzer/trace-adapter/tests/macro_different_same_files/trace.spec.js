const path = require('path');
let action = (runtime) => {
    const filePath = path.join(__dirname, 'sources', `m_dep.move`);
    let res = '';
    // step into a function, which immediately step in to a macro,
    // and then the inner macro
    runtime.step(false);
    res += runtime.toString();
    // step to leave the inner macro
    runtime.step(false);
    res += runtime.toString();
    // set a breakpoint in the inner macro and continue to hit it
    runtime.setLineBreakpoints(filePath, [4]);
    runtime.continue();
    res += runtime.toString();
    return res;
};
run_spec(__dirname, action);
