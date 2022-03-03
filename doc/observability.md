# Logging, Tracing, Metrics, and Observability

Good observability facilities will be key to the development and productionization of Sui.  This is made
more challenging by the distributed and asynchronous nature of Sui, with multiple client and authority
processes distributed over a potentially global network.

The observability stack in Sui is based on the Tokio [tracing](https://tokio.rs/blog/2019-08-tracing) library.
The rest of this document highlights specific aspects of achieving good observability such as useful logging
and metrics in Sui.

## Contexts, Scopes, and Tracing Transaction Flow

The main idea of logging and tracing in a distributed and asynchronous context, where you cannot rely on looking
at individual logs over time in a single thread, is to assign context to logging/tracing so that we can trace,
for example, individual transactions.  Context uses key-value pairs so that we can easily filter on subsets
and examine individual flows, across both thread/task and process boundaries.

Here is a table/summary of context information that we will want:

- TX Digest
- Object reference/ID, when applicable
- Address/account
- Certificate digest, if applicable
- For Client HTTP endpoint: route, method, status
- Epoch
- Host information, for both clients and authorities

Example output which shows both context (tx digests) and key-value pairs enhancing observability/filtering:

```
2022-03-03T18:44:13.579135Z DEBUG test_native_transfer:process_cert{tx_digest=t#82fd2ee00ab4d23c498369ff4d6fc0fc1a74be6f56a6d0490022e0920577f4c7}: sui_core::authority_aggregator: Received effects responses from authorities num_unique_effects=1 bad_stake=2
2022-03-03T18:44:13.579165Z DEBUG test_native_transfer:process_cert{tx_digest=t#82fd2ee00ab4d23c498369ff4d6fc0fc1a74be6f56a6d0490022e0920577f4c7}: sui_core::authority_aggregator: Found an effect with good stake over threshold good_stake=4
```

## Key-Value Pairs Schema

### Span names

Spans capture not a single event but an entire block of time, so start, end, duration, etc. can be captured
and analyzed for tracing, performance analysis, etc.

|     Name     |       Place        |                                 Meaning                                 |
| ------------ | ------------------ | ----------------------------------------------------------------------- |
| process_tx   | Gateway, Authority | Send transaction request, get back 2f+1 signatures and make certificate |
| process_cert | Gateway            | Send certificate to authorities to execute transaction                  |
| handle_cert  | Gateway, Authority | Handle certificate processing and Move execution                        |
| sync_cert    | Gateway, Authority | Gateway-initiated sync of data to authority                             |
|              |                    |                                                                         |

### Tags - Keys

The idea is that every event and span would get tagged with key/value pairs.  Events/logs that log within any context or nested contexts would also inherit the context-level tags.
These tags represent "fields" that can be analyzed and filtered by.  For example, one could filter out broadcasts and see the errors for all instances where the bad stake exceeded a certain amount, but not enough for an error.

TODO: see if keys need to be scoped by contexts

|        Key         |      Place(s)      |                                  Meaning                                   |
| ------------------ | ------------------ | -------------------------------------------------------------------------- |
| tx_digest          | Gateway, Authority | Hex digest of transaction                                                  |
| quorum_threshold   | Gateway            | Numeric threshold of quorum stake needed for a transaction                 |
| validity_threshold | Gateway            | Numeric threshold of maximum "bad stake" from errors that can be tolerated |
| num_errors         | Gateway            | Number of errors from authorities broadcast                                |
| good_stake         | Gateway            | Total amount of good stake from authorities who answered a broadcast       |
| bad_stake          | Gateway            | Total amount of bad stake from authorities, including errors               |
| num_signatures     | Gateway            | Number of signatures received from authorities broadcast                   |
| num_unique_effects | Gateway            | Number of unique effects responses from authorities                        |
|                    |                    |                                                                            |

## Logging Levels

This is always tricky, to balance the right amount of verbosity especially by default, but keeping in mind this is a high performance system.

| Level |                                              Type of Messages                                              |
| ----- | ---------------------------------------------------------------------------------------------------------- |
| Error | Process-level faults (not transaction-level errors, there could be a ton of those)                         |
| Warn  | Unusual or byzantine activity                                                                              |
| Info  | High level aggregate stats. Major events related to data sync, epoch changes.                              |
| Debug | High level tracing for individual transactions. Eg Gateway/client side -> authority -> Move execution etc. |
| Trace | Extremely detailed tracing for individual transactions                                                     |
|       |                                                                                                            |

## Metrics

## Live Async Inspection / Tokio Console

## (Re)Configuration

TODO: Discuss live changing of logging levels for example

## Viewing Logs, Traces, Metrics