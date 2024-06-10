# Sui Causality Log

Sui causality log is a system for declaring causal connections between events as they occur, either across nodes or within a single node.

## Example usage:

As an example, we will show how to annotate the causal relationships involved in checkpoint certification.

Checkpoint certification involves validators signing checkpoints that they have built locally, and distributing the signatures over the checkpoint summaries to the rest of the committee. So, we might begin by creating an event just before sending a checkpoint signature:

```
event!(
    "send_checkpoint_sig" {
        source = self.authority,
        seq = checkpoint_seq,
    }
);
```

Later, when we receive a signature, we can note the cause of the event, in order to establish the causal link:

```
let authority = signature.summary.auth_sig().authority;
let stake = self.epoch_store.committee().stake_of_authority(authority);
let seq = signature.summary.sequence_number;
event!(
    "receive_checkpoint_sig" {
        stake = stake,
        seq = seq,
    }
    caused_by "send_checkpoint_sig" {
        source = authority,
        seq = seq,
    }
);
```

However, in order to certify a checkpoint, we need to gather many signatures. When we attempt to certify the next checkpoint, we can indicate that we are expecting a quorum of signatures:

```
event!(
    "checkpoint_sig_quorum" {
        seq = current.summary.sequence_number,
    }
    caused_by "receive_checkpoint_sig" {
        required_stake = QUORUM_THRESHOLD,
        seq = current.summary.sequence_number,
    }
);
```

Finally, the checkpoint certification can be declared as

```
event!(
    "checkpoint_certified" {
        seq = summary.sequence_number,
    }
    caused_by = "checkpoint_sig_quorum" {
        seq = summary.sequence_number,
    }
);
```

Note that events can be declared either when they occur, or as soon as they are expected to occur.

Declaring events and their causes allows us to do the following things:
- When an event is declared before it has happened (as in the case of checkpoint sig aggregation), we can notice that the cause (or causes) are absent. By highlighting the absence of such causes, we may be able to quickly diagnose loss-of-liveness problems.
- By noticing that an event never causes any later events, we may be able to find other problems. For instance, if the `receive_checkpoint_sig` event never occurs, we can notice that the `send_checkpoint_sig` event is "dangling". This may indicate (for instance) that consensus has stalled.
- By consuming an entire log of events, we could visualize the entire causal flow of Sui during some period of time (this will require a lot of work to build the visualization tools).

## Details

A few notes about events:

- Events are uniquely identified by their tags, and can only occur once. So, repeatedly logging an event with the same tags will only be counted once. This makes it easy to add logging to code that repeatedly attempts to do a task until it finishes without worrying about creating multiple events.
- Several tag names are reserved, and treated specially:
  - `source` - The node that is emitting the event. Should be an AuthorityName. If omitted, the event is consider local, and will only be linked to later events that occur on the same node.
  - `stake` - The amount of stake represented by the event. Cannot appear in `caused_by`.
  - `required_stake` - The amout of stake, summed across all causes with the same tags, required for an event to be considered causes. Can only appear in `caused_by`.

## Deployment

Initially, the causality log is intended to be used as a local test debugging aid, by consuming the log output of a simtest run (which has all the logs from all nodes in a single file).

However, the log system is designed to support very low overhead logging in a live network. Eventually, validators may keep a circular buffer of event logs in their memory. In the event of a network stall, we could fetch the event logs from the validators and hopefully diagnose the issue quickly.

## Usage

WIP: This section describes intended usage, none of this works yet

To analyze a log file, run:

     cargo run -p sui-causality-log --bin analyzer <log file>