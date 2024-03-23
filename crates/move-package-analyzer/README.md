# Move Package Analyzer
Load all packages of a network (mainnet, testnet, ...) and build relationship across them.
That allows for different kind of analysis to take place.

## Usage
The tool can either read all packages from a DB or `sui-tool dump-packages` can be used to
download all packages and operate on them. Downloading all packages is the recommended way
to use this tool. It will not repeatedly access the remote DB.
In order to use `sui-tool dump-packages` you will need a proper DB url with correct privileges.
Please ask #data-platform on slack for help on how to access the DB.

The tool runs a set of passes as defined by the `passes.yaml` file.
Adding a pass is not the most straightforward thing to do, and should be simplified over time, 
however here are the steps:
1. Add a new pass in the `passes` directory. A pass has a function `pub fn run(env: &GlobalEnv, output: &Path)`
that is called by the `pass_manager.rs`. The added pass/file will contain the logic of the analysis.
2. Create a new variant in the `Pass` enum in `lib.rs` and add it to `pass_manager.rs` in the `run` function.
Call the new pass from the pass manager.
3. Add the new pass to the `passes.yaml` file.
4. Run the tool




