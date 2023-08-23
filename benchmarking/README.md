# Benchmarking with Artificially Generated Workloads

The scripts in this directory allow easy generation of artificial workloads of
SUI transfers and could easily be extended to more general workloads by changing
`workload_generation.ts` accordingly.

**Note: The binaries `sui` and `sui-faucet` need to be available in this
directory. They are not currently being built automatically due to
incompatibilities with the optimizations in this branch. They should be manually
built, e.g. from
`https://github.com/mystenLabs/sui/tree/igor/fullnode-epoch-tps`, and placed in
the root of this directory.**

## Usage

Step 1: Create output directories and compile `simple_channel_executor`. This
can be done by running the following shell script:

`./prepare.sh`

Step 2: Change the parameters in the `workload_generation.ts` file, that define
the specifics for the workload you want to generate.

Step 3: Generate the workload with an arbitrary given name, here
`simple_16_1000` is used as a shorthand for the parameters that we configured in
the previous step. This command will use the `sui` and `sui-faucet` binaries to
run a local cluster of Sui validators and a client using the Typescript SDK to
introduce transactions into the cluster.

`./generate.sh simple_16_1000`

Step 4: Run a previously generated benchmark with the following command. This
will run the `simple_channel_executor` on the generated workload.

`./run.sh simple_16_1000`
