const path = require('path');
let action = (runtime) => {
    const pkgName = path.basename(__dirname);
    const filePath = path.join(__dirname, 'build', pkgName, 'disassembly', `m2.mvb`);
    let res = '';
    runtime.setLineBreakpoints(filePath, [ 12 ]);
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
    // continue until you reach breakpoint at the end of the caller function
    runtime.continue();
    res += runtime.toString();
    return res;
};
run_spec(__dirname, action);
