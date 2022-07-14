The system allows scaling though authorities devoting more resources,
namely CPUs, memory, network and storage within a machine or over
multiple machines, to the processing of transactions. More resources
lead to an increased ability to process transactions, leading to
increased fees income to fund these resources. More resources also
results in lower latency, as operations are performed without waiting
for necessary resources to become available.

**Throughput.** To ensure that more resources result in increased
capacity quasi-linearly, the design aggressively reduces bottlenecks and
points of synchronization requiring global locks within authorities.
Processing transactions is cleanly separated into two phases, namely (1)
ensuring the transaction has exclusive access to the owned or shared
objects at a specific version, and (2) then subsequently executing the
transaction and committing its effects.

Phase (1) requires a transaction acquiring distributed locks at the
granularity of objects. For owned objects this is performed trough a
reliable broadcast primitive, that requires no global synchronization
within the authority, and therefore can be scaled through sharding the
management of locks across multiple machines by $\objectid$. For
transactions involving shared objects sequencing is required using a
consensus protocol, which does impose a global order on these
transactions and has the potential to be a bottleneck. However, recent
advances on engineering high-throughput consensus protocols [@narwhal]
demonstrate that sequential execution is the bottleneck in state machine
replication, not sequencing. In , sequencing is only used to determine a
version for the input shared object, namely incrementing an object
version number and associating it with the transaction digest, rather
than performing sequential execution.

Phase (2) takes place when the version of all input objects is known to
an authority (and safely agreed across authorities) and involves
execution of the Move transaction and commitment of its effects. Once
the version of input objects is known, execution can take place
completely in parallel. Move virtual machines on multiple cores or
physical machines read the versioned input objects, execute, and write
the resulting objects from and to stores. The consistency requirements
on stores for objects and transactions (besides the order lock map) are
very loose, allowing scalable distributed key-value stores to be used
internally by each authority. Execution is idempotent, making even
crashes or hardware failures on components handling execution easy to
recover from.

As a result, execution for transactions that are not causally related to
each other can proceed in parallel. Smart contract designers may
therefore design the data model of objects and operations within their
contracts to take advantage of this parallelism.

Check-pointing and state commitments are computed off the critical
transaction processing path to not block the handling of fresh
transactions. These involve read operations on committed data rather
than requiring computation and agreement before a transaction reaches
finality. Therefore they do not affect the latency or throughput of
processing new transactions, and can themselves be distributed across
available resources.

Reads can benefit from very aggressive, and scalable caching.
Authorities sign and make available all data that light clients require
for reads, which may be served by distributed stores as static data.
Certificates act as roots of trust for their full causal history of
transactions and objects. State commitments further allow for the whole
system to have regular global roots of trust for all state and
transactions processed, at least every epoch or more frequently.

**Latency.** Smart contract designers are given the flexibility to
control the latency of operations they define, depending on whether they
involve owned or shared objects. Owned objects rely on a reliable
broadcast before execution and commit, which requires two round trips to
a quorum of authorities to reach finality. Operations involving shared
objects, on the other hand, require a a consistent broadcast to create a
certificate, and then be processed within a consensus protocol, leading
to increased latency (4 to 8 round trips to quorums as of [@narwhal]).
