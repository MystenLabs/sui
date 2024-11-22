const path = require('path');

let action = (runtime) => {
    const filePath = path.join(__dirname, 'sources', `m_dep.move`);
    let res = '';
    runtime.setLineBreakpoints(filePath, [4]);
    // continue to the breakpoint set in the macro
    runtime.continue();
    res += runtime.toString();
    return res;
};
run_spec(__dirname, action);
