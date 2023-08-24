# Sui Mainnet Parallelizability Study

This project analyzes access patterns of the transactions on the Sui mainnet in
regards to parallelizability. It fetches transaction data from a Sui full node,
ideally (and by default) it expects the node to be running locally on the same
machine. Based on transaction data, specificially their input objects and
whether these are mutably or immutably accessed, dependency graphs are
generated. From these, together with gas cost as a proxy for execution time,
possible speedups can be calculated. The hypothetical parallel scheduling is
performed within each checkpoint.

## Generating Data

The main executable can be run as follows:

```sh
cargo run --release
```

By default it runs over the entire history of mainnet. It has two important
optional command line parameters (`from-epoch` and `to-epoch`), which can be
used to limit the timeframe of the data under analysis. For extending the data
from epoch 0 through 50 up to epoch 80 one could run the following and append
the new CSV data to the previous file accordingly.

```sh
cargo run --release -- --from-epoch 50 --to-epoch 80
```

Importantly, there are three different modes of operation `Scheduling`,
`Accesses`, and `Graph`. These can be selected by changing the `enum` constant
called `MODE` in the code.

Firstly, `Scheduling` mode is the most important one and produces the main
parallelizability metrics, which are _total gas per checkpoint_ and _sequential
gas per checkpoint_. The second one is built by running a simple list scheduling
algorithm on the transaction in commit order given their dependencies. This
output is in CSV format.

Secondly, `Accesses` mode produces for each **transaction** the number of
accesses split by type of the access (i.e. object type and mutable/immutable).
This output is in CSV format. The `accesses.py` script can be used to calculate
averages over one of those CSV outputs.

Finally, `Graph` mode is only meant to be used to enhance data produced by
`Scheduling` with information about the cliques of the dependency graphs. On its
own it only outputs the dependency graph in a simple text format, i.e. nodes and
eges each on a single line. This output can however be post-processed with the
`cliques.py` script, which uses a highly efficient heaviest clique algorithm to
find the heaviest clique in each of the graphs.

## Plotting

For plotting there are two options:

### Python + matplotlib

Running the Python script `plot.py` assumes the file `./data/scheduling.csv` to
exist and for it to contain data in the format of the output of `Scheduling`
mode (see above). It then produces and displays two plots, one of the possible
speedups implied by the dependency graph characteristics, and one of the general
evolution of the workload size, both as a development over time.

### Rust + charming

Running the cargo project in the `plot_test` directory produces a similar plot
of the scheduling data. It also assumes the file `./data/scheduling.csv` to
exist and it produces the plot as `./scheduling.svg`.
