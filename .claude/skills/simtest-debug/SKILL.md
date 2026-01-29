# Debug a simtest failure

Debugs a simtest failure using logging and the scientific method.

## Usage

```
/debug-simtest <repro command>
```

Example: `/debug-simtest "MSIM_TEST_SEED=1768248386016 RUST_LOG=sui=debug,info cargo simtest --test address_balance_tests test_deposit_and_withdraw"`

## Arguments

$ARGUMENTS should contain a single quoted string which is a command to run to reproduce the test failure.

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

Execute the following steps:

### 1. Ask the user for a description of the failure and any additional useful context.

### 2. Run the test.
Run the test as given in the commandline. If `RUST_LOG=...` is missing, add `RUST_LOG=sui=debug,info`. if `--no-capture` is missing, add it.
Redirect the test output to a file. Do not run the test in the background or use a timeout. It may run for a long time, but it will finish.

### 3. Examine the output and make observations.
Use grep and other tools to examine the output log (which will be very large). Summarize your observations to NOTEBOOK.md.

### 4. Form a hypothesis

Based on the observations, form a hypothesis. Check if the hypothesis has not been ruled out by prior experiments. record it to NOTEBOOK.md.

### 5. Plan an experiment

An "experiment" consists of adding logging statments to the code which can confirm or refute the hypothesis. All logging statements should be of the form `info!("CLAUDE: ...")` so that you can grep for them easily.
Summarize the experiment to NOTEBOOK.md

### 6. Run and evaluate the experiment
after adding logs to the code, run the reproduction command again. Determine whether the hypothesis was confirmed or refuted by the observations.
Record the results of the experiment to NOTEBOOK.md.

### 7. Decide whether we have found the root cause
if a hypothesis has been confirmed, determine if it is the root cause.  If so, the debugging is complete. Summarize the results to the user.

Otherwise, if the hypothesis is refuted, or if it is not a root cause, return to step 3, and form a new hypothesis consistent with previous hypotheses.  Repeat all steps until we find the root cause.
