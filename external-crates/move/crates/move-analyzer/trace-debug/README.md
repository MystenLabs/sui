# Move Trace Debug

Provides the ability to visualize Move trace files, which can be generated for a given package when running Move tests. These trace files contain information about which Move instructions are executed during a given test run. This extenstion leverages an implementation of the [Debug Adapter Protocol](https://microsoft.github.io/debug-adapter-protocol) (DAP) that analyzes Move execution traces and presents them to the IDE client (in this case a VSCode extension) in a format that the client understands and can visualize using a familiar debugging interface.

# How to Install

When this extension and its companion DAP implementation become more mature, we will make it available in the VSCode Marketplace. At the moment, in order to experiment with it must be built and installed locally (start in the main directory of this extension):
1. Install dependencies for the extension:
```bash
npm install
```
2. Install dependencies for the DAP implementation:
```bash
npm install --prefix ../move-trace-adapter
```
3. Package the extension (it will pull relevant files from the DAP implementation)
```
vsce package -o move-trace-debug.vsix
```
4. Install move-trace-debug.vsix in VSCode using `Install from VSIX...` option
