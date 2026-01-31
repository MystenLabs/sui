# Debug a simtest failure

Debugs a simtest failure using logging and the scientific method.

## Usage

```
/debug-simtest <repro command or test>
```

Example: `/debug-simtest MSIM_TEST_SEED=1768248386016 RUST_LOG=sui=debug,info cargo simtest --test address_balance_tests test_deposit_and_withdraw`
Example: `/debug-simtest test_deposit_and_withdraw

## Arguments

$ARGUMENTS should contain either:
- a command to run to reproduce the test failure
- the name of a test

## Instructions

This skill logs its progress to a file called NOTEBOOK.md in the repo root. The file should look like

```
# iteration 1
- OBSERVATIONS
  - <observation 1>
  - <observation 2>
  - ...
- HYPOTHESIS: <description of hypothesis>
- EXPERIMENT: <description of experiment>
- RESULTS: <did the experiment confirm or refute the hypothesis?>

# iteration 2
<more observations, hypothesis, etc>
```

While executing the skill, never make functional changes to the code.  Only add logging statements as described below.  The goal is to find the root cause, not fix the issue.
If at any point the reproduction command stops failing, or fails in a different way, this indicates that functional changes have been made. The test is deterministic and cannot
be affected by emitting logs, doing expensive computations, etc.

## Log File Management

Log files should be named consistently using the experiment number: `experiment_N.log` (e.g., `experiment_1.log`, `experiment_2.log`, etc.).
- Never delete log files during the debugging session - the user will clean them up when debugging is complete.
- Do not commit log files to git - they are too large.

## Commit Strategy

After each experiment:
1. Commit only the logging changes (code modifications) with a message indicating the experiment number: `CLAUDE: experiment N logging`
2. Make a separate commit for NOTEBOOK.md updates with a message like `CLAUDE: experiment N observations`

This keeps the debugging history clean and allows easy navigation between experiments. Do not include log files in any commits.

Execute the following steps:

### 1. Ask the user for a description of the failure and any additional useful context.

### 2. Find a reproduction of the test failure

If a full command was provided, this command is the repro.

If only a test name was provided, find a repro as follows:

- determine the test target in which the test is defined. Usually it will be in an integration test target, not a unittest.
- run `./scripts/simtest/seed-search.py --test <test_target_name> <test_name> --exact`. This program will search for a seed that fails.
- Once you have a seed that fails, the repro will be `MSIM_TEST_SEED=<seed> cargo simtest --test <test_target> -E 'test(=<test_name>)'`

### 3. Run the test.

Run the repro command. If `RUST_LOG=...` is missing, add `RUST_LOG=sui=debug,info`. if `--no-capture` is missing, add it.
Redirect the test output to a file named `experiment_N.log` where N is the iteration number. Do not run the test in the background or use a timeout. It may run for a long time, but it will finish.

### 4. Examine the output and make observations.
Use grep and other tools to examine the output log (which will be very large). Summarize your observations to NOTEBOOK.md.

### 5. Form a hypothesis

Based on the observations, form a hypothesis. Check if the hypothesis has not been ruled out by prior experiments. record it to NOTEBOOK.md.

### 6. Plan an experiment

An "experiment" consists of adding logging statments to the code which can confirm or refute the hypothesis. All logging statements should be of the form `info!("CLAUDE: ...")` so that you can grep for them easily.
Summarize the experiment to NOTEBOOK.md

### 7. Run and evaluate the experiment
After adding logs to the code (N is the iteration number):
1. Commit the logging changes with message `debug: experiment N logging` (do not include log files)
2. Run the reproduction command, redirecting output to `experiment_N.log`
3. Determine whether the hypothesis was confirmed or refuted by the observations
4. Record the results of the experiment to NOTEBOOK.md
5. Commit the NOTEBOOK.md updates with message `debug: experiment N observations`

### 8. Decide whether we have found the root cause
if a hypothesis has been confirmed, determine if it is the root cause.  If so, the debugging is complete. Summarize the results to the user.

Otherwise, if the hypothesis is refuted, or if it is not a root cause, return to step 4, and form a new hypothesis consistent with previous hypotheses.  Repeat all steps until we find the root cause.

### 9. After the root cause has been found, discuss possible fixes with the user, and implement one if requested.

### 10. After the fix is implemented, return to step 2 and use seed-search.py to see if there are still other failing seeds.

## Cleanup Before PR

When creating a PR for the fix:
1. **Remove all debugging commits** - The debug commits (logging changes, NOTEBOOK.md updates) should NOT be included in the PR. Use `git rebase` to remove them.
2. **Delete NOTEBOOK.md** - This file is for debugging only.
3. **Delete log files** - Remove all `experiment_N.log` files.
4. The PR should contain ONLY the actual fix, with a clean commit history.
