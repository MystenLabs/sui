const path = require('path');
let action = (runtime) => {
    const filePath = path.join(
        __dirname,
        '35fsf',
        'bytecode',
        'f2c3d',
        'global_assign_ref.mvb'
    );
    console.log(filePath);
    let res = '';
    runtime.setLineBreakpoints(filePath, [ 46, 48 ]);
    // execute until before WRITE_REF instruction
    // that may have incorrectly processed effects
    runtime.continue();
    res += runtime.toString();
    // execute until after WRITE_REF instruction
    // that may have incorrectly processed effects
    // where variables on the stack should still
    // be displayed correctly
    runtime.continue();
    res += runtime.toString();

    return res;
};
run_spec_replay(__dirname, action);
