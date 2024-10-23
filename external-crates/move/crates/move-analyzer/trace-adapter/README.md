# Move Trace Adapter

Implements [Debug Adapter Protocol](https://microsoft.github.io/debug-adapter-protocol) (DAP) to visualize Move VM traces using a familiar debugging interface. It is a self-contained package in that rather than providing just the adapter that needs to connect to an actual debugger, it implements both the "adapter" part (responsible for communicating with an IDE that can understand DAP) and the "visualizer" part (responsible for analyzing traces and maintaining a runtime to maintain the visualizer/debugger state).

# Features

The feature set currently includes (new features upcoming!):
- forward "step" action: step to next expression and into a regular Move function call
- "step out" action: step out of the current function call into the outer one
- "next" action: step over a function call (instead of steppig into it)
