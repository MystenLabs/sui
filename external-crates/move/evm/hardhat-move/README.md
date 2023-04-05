# hardhat-move

## What
This is an hardhat plugin that adds support for the Move language.
This plugin extends the `compile` task with a sub task to compile contracts written in Move and generate the require artifacts for testing and deployment.

## Setting up the Plugin
Right now this plugin is still experimental and requires some manual steps to set up.

Step 1: compile and install the `move` executable
```
cd language/tools/move-cli
cargo install --path .
```

Step 2: set up the dev environment for `hardhat-move`
```
cd language/evm/hardhat-move
npm install
```

Step 3: since `hardhat-move` is written in typescript, it needs to be compiled
```
cd language/evm/hardhat-move
npm run build
```

With these steps done, you should be able to use this plugin in `hardhat-examples`, provided that you have already set that up. For now, if you wish to use `hardhat-move` in another hardhat project, you would have to add it as a dependency to `package.json` located in the root of your project, rerun `npm install`, and add `require("hardhat-move");` to the top of your `hardhat.config.js`, similar to what `hardhat-examples` had done.

## Writing Contracts in Move
Move contracts should adhere to the following directory layout
```
<hardhat project root>
    - contracts
        - MyMovePackage1
            - sources
            - Move.toml
        - MyMovePackage2
            - sources
            - Move.toml
```
Currently, exactly one contract is generated from each Move package, with the **contract name equal to the package name**. It should noted that this is more of a tentative design and we may add a finer way for the user to specify the package/module-to-contract mapping, potentially allowing custom contract names and defining multiple contracts in the same package.
