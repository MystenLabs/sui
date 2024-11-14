# Verification summary
* `deposit`:
    - time: 4m
    - command: `boogie -doModSetAnalysis -vcsCores:12 -verifySeparately -vcsMaxKeepGoingSplits:10 -vcsSplitOnEveryAssert -vcsFinalAssertTimeout:600 output.bpl`
* `withdraw`:
    - time: 45s
    - command: `boogie -doModSetAnalysis -vcsCores:12 -verifySeparately -vcsMaxKeepGoingSplits:10 -vcsSplitOnEveryAssert -vcsFinalAssertTimeout:600 output.bpl`
* `calc_swap_result`:
    - time: 90s
    - command: `boogie -doModSetAnalysis -vcsCores:12 -verifySeparately -vcsMaxKeepGoingSplits:10 -vcsSplitOnEveryAssert -vcsFinalAssertTimeout:600 output.bpl`
* `create`, `swap_a`, `swap_b`:
    - time: 10s
    - command: `sui-move build --generate-boogie`
