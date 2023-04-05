This package contains examples for programming patterns in Async Move (code name AMV). We use a simple
account, where accounts live on different actors. One can deposit, withdraw, and transfer
between those accounts. The transfer is the interesting operation because it requires a roundtrip
communication between actors: only if deposit on end is successful, should the money be withdrawn
on the other.

Note that there is a conceptual bug in this solution: since multiple transfers can be initiated simultaneously,
but the money is not withdrawn before one finishes, the account balance could get negative (similar as the
reentrancy problem in Solidity). We chose to ignore this for the sake of illustrating the communication patterns.

Note also that the solutions assume reliable messaging (exactly-once semantics).

There are three versions of this example:

- With continuations.
- With futures.
- With plain message passing and an explicit state machine.
