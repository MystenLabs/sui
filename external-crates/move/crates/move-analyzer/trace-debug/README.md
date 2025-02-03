# Move Trace Debugging

Provides the ability to visualize Move trace files, which can be generated for a given package when running Move tests. These trace files contain information about which Move instructions are executed during a given test run. This extension leverages an implementation of the [Debug Adapter Protocol](https://microsoft.github.io/debug-adapter-protocol) (DAP) that analyzes Move execution traces and presents them to the IDE client (in this case a VSCode extension) in a format that the client understands and can visualize using a familiar debugging interface.

## Supported features

Currently we support trace-debugging of Move unit tests only. and the following trace-debugging features are supported:
- stepping forward through the trace (step, next, step out, and continue commands)
- tracking local variable values (including enums/structs and references)
- line breakpoints
- disassembly view (disassembled bytecode view) with support for stepping and setting breakpoints in disassembled bytecode files

Note that support for trace-debugging macros and enums is limited at this point - stepping through macros or code related to enums may result in somewhat unexpected results due to how these constructs are handled internally by the Move execution framework. In particular, variable value tracking may be affected when trace-debugging these constructs. Work is ongoing to improve state-of-the-art - improvement suggestions and bug reports files as issues against Sui's GitHub [repository](https://github.com/MystenLabs/sui) are greatly appreciated.

# How to Install

1. Open a new window in any Visual Studio Code application version 1.61.0 or greater.
2. Open the command palette (`⇧` + `⌘` + `P` on macOS, `^` + `⇧` + `P` on Windows and GNU/Linux,
   or use the menu item *View > Command Palette...*) and
   type **Extensions: Install Extensions**. This will open a panel named *Extensions* in the
   sidebar of your Visual Studio Code window.
3. In the search bar labeled *Search Extensions in Marketplace*, type **Move Trace Debugger**. The Move Trace debugger extension
   should appear as one of the option in the list below the search bar. Click **Install**.

# How to trace-debug a Move unit test

Debugging a Move unit tests consists of two steps: generating a Move trace and actually trace-debugging it.

## Generating a Move trace

If you have [Mysten's Move extension](https://marketplace.visualstudio.com/items?itemName=mysten.move) installed you can generate a Move trace for tests defined in a given file by navigating to this file in VSCode and running `Move: Trace Move test execution` from the command palette. See the description of [Mysten's Move extension](https://marketplace.visualstudio.com/items?itemName=mysten.move) for pre-requisites needed to run this command.

If you plan to use the Trace Debugging Extension by itself, you need to generate the traces using command-line interface of `sui` binary. See [here](https://docs.sui.io/guides/developer/getting-started/sui-install) for instructions on how to install `sui` binary. Note that the `sui` binary must be built with the `tracing` feature flag. If your version of the `sui` binary was not built with this feature flag, an attempt to trace test execution will fail. In this case you may have to build the `sui` binary from source following these [instructions](https://docs.sui.io/guides/developer/getting-started/sui-install#install-sui-binaries-from-source).

Once the `sui` binary is installed, you generate traces for all test files in a given package, as well as disassembled bytecode for all the modules (to support disassembly view) by running the following command in the package's root directory:
```shell
sui move test --trace-execution
```

You can limit trace generation to the tests whose name contains a filter string by passing this string as an additional argument to the trace generation command:
```shell
sui move test FILTER_STRING --trace-execution
```

## Trace-debugging a test

Once traces are generated, open a Move file containing the test you want to trace-debug and execute `Run->Start Debug` command. The first time you execute this command, you will have to choose the debugging configuration for Move files, of which there should be only one available. Then you will have to choose a test to trace-debug if there is more than one test in a file (otherwise a trace-debugging session for a single test will start automatically). You can switch between source (regular) view and the disassembly view (where you can inspect and step through disassembled bytecode for lower-level view for Move code execution) by using **Move: Toggle disassembly view** and **Move: Toggle source view** commands from VSCode's command palette.
