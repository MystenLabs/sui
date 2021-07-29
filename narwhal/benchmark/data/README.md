# Experimental Data
This folder contains some raw data and plots obtained running a geo-replicated benchmark on AWS as explained in the [benchmark's readme file](https://github.com/facebookresearch/narwhal/tree/master/benchmark#readme).

The filename format of raw data is the following:
```
bench-FAULTS-NODES-WORKERS-COLLOCATE-INPUT_RATE-TX_SIZE.txt
```
where:
- `FAULTS`: The number of faulty (dead) nodes.
- `NODES`: The number of nodes in the testbed.
- `WORKERS`: The number of workers per node.
- `COLLOCATE`: Whether the primary and its worker are collocated on the same machine.
- `INPUT_RATE`: The total rate at which clients submit transactions to the system.
- `TX_SIZE`: The size of each transactions (in bytes).

For instance, a file called `bench-0-50-1-True-100000-512` indicates it contains results of a benchmark run with 50 nodes, 1 worker per node collocated on the same machine as the primary, 100K input rate, a transaction size of 512B, and 0 faulty nodes.