---
name: Move IDE Issue
about: Create a new report for issues encountered running Move IDE
title: Move IDE issue report'
labels: move-ide
assignees: 'awelc'
---

## IDE Setup

Describe your setup:
* OS: <specify OS version>
* IDE/editor: <specify IDE/editor version>
* Move analyzer version: <specify `move-analyzer` version>

When using Move VS Code [extension](https://marketplace.visualstudio.com/items?itemName=mysten.move), make sure to use the most recent version of both VS Code (or equivalent editor, such as Cursor) and the Move VS Code extension. Additionally, provide the content of the **Move Client** tab. To access this data, select **View** -> **Output** from the main menu and open the appropriate tab from the drop down menu. The output should look similar to the following:

``` shell
INFO [10/21/2025, 10:16:14 AM]: mysten.move version 1.0.33
INFO [10/21/2025, 10:16:14 AM]: Creating extension context
INFO [10/21/2025, 10:16:14 AM]: configuration: {"auto-imports":true,"force-bundled":false,"inlay-hints":{"type":false,"param":false},"lint":"default","server":{"path":null},"sui":{"path":"/opt/homebrew/bin/sui"},"trace":{"server":"off"}}
INFO [10/21/2025, 10:16:14 AM]: Installing language server
INFO [10/21/2025, 10:16:14 AM]: Bundled version: 1.60.0
INFO [10/21/2025, 10:16:14 AM]: Standalone version: 1.60.0
INFO [10/21/2025, 10:16:14 AM]: CLI version: 1.58.2
INFO [10/21/2025, 10:16:14 AM]: Setting v1.60.0 of standalone move-analyzer installed at '~/.sui/bin/move-analyzer' as the highest one
INFO [10/21/2025, 10:16:14 AM]: Starting client...
```

When using a different editor (e.g., Vim, Emacs), provide its version and version of `move-analyzer` binary your editor is using. You can get `move-analyzer` version by running the following command:

``` shell
move-analyzer --version
```

## Steps to Reproduce Issue

Provide the concrete steps needed to reproduce the issue. The more detail you provide, the better chance the problem can be addressed. If the issue is not reproducible, skip this step and proceed to the following ones. When providing code in the reproduction steps, use the smallest buildable example that demonstrates the issue, removing any extraneous details.

e.g.
1. Clone repository <repository>
1. Load file <file>.
2. Hover over language construct <construct> on line <line> in column <column>


## Expected Result

Specify what outcome you expected should have resulted, but didn't.

e.g.
Expected some on-hover information to appear when hovering over <construct> on line <line> in column <column>

## Actual Result

Specify what the actual unexpected outcome was.

e.g.
No on-hover information was displayed when hovering over <construct> on line <line> in column <column>

## Editor Logs

Upon encountering and issue, capture `move-analyzer` logs that may help diagnosing the issue. 

When using the Move VS Code extension, provide the content of the **Move** tab. To access this data, select **View** -> **Output** from the main menu and open the appropriate tab from the drop down menu. Beginning of the log should look similar to the following:

``` shell
using standard allocator
Starting language server '~/.sui/bin/move-analyzer' communicating via stdio...
linting level Default
parent process monitoring enabled for PID: 31866
auto imports during auto-completion enabled: true
inlay type hints enabled: false
inlay param hints enabled: false
starting symbolicator runner loop
text document notification
scheduling run for "~/deepbookv3/packages/deepbook/sources/balance_manager.move"
scheduled run
text document notification handled
symbolication started
symbolicating "~/deepbookv3/packages/deepbook"
[note] Dependencies on Bridge, MoveStdlib, Sui, and SuiSystem are automatically added, but this feature is disabled for your package because you have explicitly included dependencies on Sui. Consider removing these dependencies from Move.toml.
on_document_symbol_request: "~/deepbookv3/packages/deepbook/sources/balance_manager.move"
no cached deps for "~/deepbookv3/packages/deepbook"
pre-compiling dep MoveStdlib
inserting new dep into cache for "~/.move/https___github_com_MystenLabs_sui_git_framework__mainnet/crates/sui-framework/packages/move-stdlib"
pre-compiling dep Sui
inserting new dep into cache for "~/.move/https___github_com_MystenLabs_sui_git_framework__mainnet/crates/sui-framework/packages/sui-framework"
pre-compiling dep token
inserting new dep into cache for "~/deepbookv3/packages/deepbook/../token"
compiled to parsed AST
on_document_symbol_request: "~/deepbookv3/packages/deepbook/sources/balance_manager.move"
compiled to typed AST
compiling to CFGIR
compiled to CFGIR
compilation complete in: 4.177246625s
analysis complete in 832.949458ms
get_symbols load complete
Retrying compilation for "~/deepbookv3/packages/deepbook"
symbolicating "~/deepbookv3/packages/deepbook"
Force-reading from lock file
found cached deps for "~/deepbookv3/packages/deepbook"
on_document_symbol_request: "~/deepbookv3/packages/deepbook/sources/balance_manager.move"
compilation complete in: 404.352625ms
analysis complete in 978.329291ms
get_symbols load complete
symbolication finished
```

When using a different editor, capture error output of the `move-analyzer` binary. Consult your editor's documentation to discover how to access this data.
