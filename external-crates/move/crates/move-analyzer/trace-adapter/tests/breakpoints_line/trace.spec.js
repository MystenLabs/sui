const path = require('path');
let action = (runtime) => {
    const filePath = path.join(__dirname, 'sources', `m.move`);
    let res = '';
    runtime.setLineBreakpoints(filePath, [
        10, // invalid (in if branch not traced)
        12, // valid (in traced if branch)
        14, // invalid (empty line)
        18, // valid (past loop)
        20, // valid (in loop)
        31 // valid (in a function)
    ]);
    res += runtime.toString();
    // advance to the caller
    runtime.continue();
    res += runtime.toString();
    // advance beyond the loop
    runtime.continue();
    res += runtime.toString();
    // advance into the loop
    runtime.continue();
    res += runtime.toString();
    // advance into the loop again
    runtime.continue();
    res += runtime.toString();
    // continue to a breakpoint in a function
    runtime.continue();
    res += runtime.toString();
    // step out of the function
    runtime.stepOut();
    // step over a function call to trigger a breakpoint
    runtime.step(true);
    res += runtime.toString();
    return res;
};
run_spec(__dirname, action);
