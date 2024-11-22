# Move Formatter

This is an alpha build of the Move Formatter extension. Note that this extension is not final, should not be considered a production-grade tool, and is intended only for early access testing of the Prettier formatter in Move.

# Using the formatter

After installing the extension, you should be ready to start using the formatter without installing any other tools.

To use the formatter, enter Command-Shift-P or Control-Shift-P to bring up the VSCode Command Palette, then type 'Format Document'. The default hotkey for this command in VSCode is Control-Shift-I or Command-Shift-I.

# Configuring the formatter

Extension looks for `.prettierrc` (or similar prettier configuration files) in the workspace. If a configuration file is not found, it will fallback to extension settings, and, if not, to `prettier` configuration if a separate Prettier extension is installed.

## VSCode Configuration

VSCode configuration for the extension is placed under `prettierMove`, these are the default settings of the
extension:

```json
{
	"prettierMove.tabWidth": 4,
	"prettierMove.printWidth": 100,
	"prettierMove.useModuleLabel": true,
	"prettierMove.autoGroupImports": "module",
	"prettierMove.enableErrorDebug": false,
	"prettierMove.wrapComments": false
}
```

### Option: `useModuleLabel`

Boolean. When set to `true`, will convert old module blocks to a module label. Won't be applied to files containing more than one module.

### Option: `autoGroupImports`

Possible values: `package`, `module`. When set to `module`, each module dependency will be on a separate line. When
set to `package`, dependencies will be grouped by package address.

```move
// value: `module`
use std::string::String;
use std::type_name;

// value: `package`
use std::{string::String, std::type_name};
```

# Reporting problems

This is an alpha release. While it has been tested to the best of our ability, IT IS STILL POSSIBLE TO LOSE WORK because of bugs in the formatter. Please take steps to avoid deletion of your work, including only running the formatter over code that has been committed or otherwise backed-up.

If you encounter a problem where the formatter emits malformed code or otherwise makes things worse for you, please report an issue on Github on the repo https://github.com/mystenlabs/sui/ using a title beginning with [formatter].
You can also leave a note on [Discord](https://discord.com/invite/sui) or [Telegram](https://t.me/+pxh89f8xU5RmYjNh).

# License

Apache-2.0
