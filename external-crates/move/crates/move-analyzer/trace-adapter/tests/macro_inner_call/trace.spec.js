const path = require('path');
let action = (runtime) => {
    const filePath = path.join(__dirname, 'sources', `m_dep_dep.move`);
    let res = '';
    // step into a function, which immediately step in to a macro
    runtime.step(false);
    // step into inner function
    runtime.step(false);
    res += runtime.toString();
    // step out into the macro
    runtime.stepOut();
    res += runtime.toString();
    // set a brakepoint in the inner function and continue to hit it
    runtime.setLineBreakpoints(filePath, [4]);
    runtime.continue();
    res += runtime.toString();
    return res;
};
run_spec(__dirname, action);
