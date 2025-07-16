const path = require('path');
let action = (runtime) => {
    const filePath = path.join(
        __dirname,
        'Hh6GKJ1ZQgyei6gqf2Dv74ZqUwoXAENSymvb7u6ADt3u',
         'bytecode',
         '0xd57798f09b33bdf87d7467d08eb99e545af9568fad06e7a006088641a01d922b',
         'global_assign_ref.mvb'
        );
    console.log(filePath);
    let res = '';
    runtime.setLineBreakpoints(filePath, [ 37, 39 ]);
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
