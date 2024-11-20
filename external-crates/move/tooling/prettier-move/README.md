# Prettier Move Plugin

This is a Move language plugin for the
[Prettier](https://prettier.io/) code formatter. It uses a Move
[parser](https://github.com/tzakian/tree-sitter-move) built on top of the
[tree-sitter](https://tree-sitter.github.io/) parser generator and maintained by Tim Zakian.

The plugin is platform-independent by utilizing a [WASM](https://webassembly.org/)-ified version of
the Move parser included in this repository at
[(./tree-sitter-move.wasm)](./tree-sitter-move.wasm). You can re-generate the WASM-ified version of
the parser by running the [scripts/treesitter-wasm-gen.sh](scripts/treesitter-wasm-gen.sh) script
(prerequisites for this are listed in the script itself). You should be careful when doing so, as
certain changes to the parser may break the plugin (e.g., if parse tree node types are modified).

## Prerequisites

In order to use the plugin, you need to install `npm` command (`brew install npm` on a
Mac). You can use the plugin to format Move files (`.move` extension) both on the command line and
using Prettier's VSCode
[extension](https://marketplace.visualstudio.com/items?itemName=esbenp.prettier-vscode). When the
plugin is complete, we will make it available directly from Move's VSCode extension.

## Installation

The plugin can be installed via npm:

```
npm i @mysten/prettier-plugin-move
```

## Usage

Go to the root directory of the Move package whose files you'd like to format (i.e., the directory
containing the Move.toml manifest file for this package) and run the following command:

```bash
npm install prettier@3.1.1 "$SUI"/external-crates/move/crates/move-analyzer/prettier-plugin
```

This will install both the prettier formatter and the plugin in the `./node_modules` directory.

# Command-line Usage

You can format Move files in the package where you completed the installation [step](#installation) by running the
following command:

```bash
./node_modules/.bin/prettier --plugin=prettier-plugin-move "$PATH_TO_MOVE_FILE"
```

# VSCode integration

In order to use the plugin in VSCode you first need to install Prettier's VSCode
[extension](https://marketplace.visualstudio.com/items?itemName=esbenp.prettier-vscode).

Then, in the root directory of the package where you completed the installation [step](#installation) you need to place the `.prettierrc` file containing the following configuration:

```
{
"plugins": [
    "prettier-plugin-move"
  ]
}
```

After completing these steps, if you open the root directory of the package where you completed the
installation [step](#installation) and choose any of the Move source files in this package, you will
be able to format them by choosing `Format Code` command from VSCode's command palette.

## Contribute

See [CONTRIBUTING](./CONTRIBUTING.md).
