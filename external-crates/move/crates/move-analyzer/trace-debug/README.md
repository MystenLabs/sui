# Move Trace Debugging

Provides the ability to visualize trace files, which can be generated for a given package when running Move unit tests or replaying on-chain transactions with the [replay tool](https://docs.sui.io/references/cli/replay). These trace files contain information about operations executed during a Move unit test run or during an on-chain transaction run (including [PTB](https://docs.sui.io/concepts/transactions/prog-txn-blocks) commands).

This extension leverages an implementation of the [Debug Adapter Protocol](https://microsoft.github.io/debug-adapter-protocol) (DAP) that analyzes execution traces and presents them to the IDE client (in this case a VSCode extension) in a format that the client understands and can visualize using a familiar debugging interface.

## Supported features

Currently we support inspection of native PTB commands and trace-debugging of Move code which supports the following features:
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

# How to use

A detailed description of the debugger is available as part of the Sui [documentation](https://docs.sui.io/references/ide/debugger), including comprehensive usage [instructions](https://docs.sui.io/references/ide/debugger#usage).
