# Move Debug Trace Adapter

Implements [Debug Adapter Protocol](https://microsoft.github.io/debug-adapter-protocol) (DAP) to support debugging of both Move function call traces and external event traces. It is a self-contained package in that rather than providing just the adapter that needs to connect to an actual debugger, it implements both the "adapter" part (responsible for communicating with an IDE that can understand DAP) and the "debugger" part (responsible for analyzing traces and providing a runtime to maintain the debugger state).

# Features

The feature set currently includes:
- forward "step" action: step to next expression and into a regular Move function call
- "step out" action: step out of the current function call into the outer one
- "next" action: step over a function call (instead of stepping into it)
- line breakpoints
- "continue" action - continue execution to the next breakpoint or to the end of the trace
- local variable value inspection
- source- and disassembly-level debugging of Move code (provides that Move sources and disassembled Move bytecode files are provided)
