const path = require('path');
let action = (runtime) => {
    const filePath = path.join(__dirname, 'build', 'disassembly', 'disassembly', 'm.mvb');
    let res = '';
    runtime.setCurrentMoveFileFromFrame(0);
    runtime.toggleDisassembly();
    runtime.setLineBreakpoints(filePath, [22, 30]);
    // step into a function
    runtime.step(false);
    runtime.step(false);
    // step through 3 individual instructions, even though in source
    // they are on the same line
    runtime.step(false);
    res += runtime.toString();
    runtime.step(false);
    res += runtime.toString();
    runtime.step(false);
    res += runtime.toString();
    // go two instructions forward to a breakpoint
    runtime.continue();
    res += runtime.toString();
    // step over the next 2 individual instructions even though
    // they involve entering a macro (and a related inline frame
    // (frame is still present in the stack view so that we can
    // revert to source view if we want to)
    runtime.step(true);
    res += runtime.toString();
    runtime.step(true);
    res += runtime.toString();
    // go to the next breakpoint to see internal (macro-related)
    // variables (starting with '%')
    runtime.continue();
    res += runtime.toString();
    return res;
};
run_spec(__dirname, action);
