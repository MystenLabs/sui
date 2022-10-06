# Experimental Data

This folder contains the raw data and plots used in the evaluation section of the paper [Narwhal and Tusk: A DAG-based Mempool and Efficient BFT Consensus](https://arxiv.org/pdf/2105.11827.pdf). The data are obtained running a geo-replicated benchmark on AWS as explained in the [benchmark's readme file](https://github.com/mystenlabs/narwhal/blob/main/benchmark#readme). The results are taken running the code tagged as [v0.2.0](https://github.com/asonnino/narwhal/tree/v0.2.0).

### Filename format
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

For instance, a file called `bench-0-50-1-True-100000-512.txt` indicates it contains results of a benchmark run with 50 nodes, 1 worker per node collocated on the same machine as the primary, 100K input rate, a transaction size of 512B, and 0 faulty nodes.

### Experimental step
The content of our [settings.json](https://github.com/mystenlabs/narwhal/blob/main/benchmark/settings.json) file looks as follows:
```json
{
    "key": {
        "name": "aws",
        "path": "/absolute/key/path"
    },
    "port": 5000,
    "repo": {
        "name": "narwhal",
        "url": "https://github.com/mystenlabs/narwhal",
        "branch": "master"
    },
    "instances": {
        "type": "m5d.8xlarge",
        "regions": ["us-east-1", "eu-north-1", "ap-southeast-2", "us-west-1", "ap-northeast-1"]
    }
}
```
We set the following `node_params` in our [fabfile](https://github.com/mystenlabs/narwhal/blob/main/benchmark/fabfile.py):
```python
node_params = {
    'header_size': 1_000,  # bytes
    'max_header_delay': 200,  # ms
    'gc_depth': 50,  # rounds
    'sync_retry_delay': 10_000,  # ms
    'sync_retry_nodes': 3,  # number of nodes
    'batch_size': 500_000,  # bytes
    'max_batch_delay': 200  # ms
}
```

