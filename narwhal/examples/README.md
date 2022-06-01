## How to run the demo client
## Via fabric

1. Go to the `benchmark` folder 
2. Install the requirements 

```
pip install -r requirements.txt
```

3. Run the `demo` fabric command

```
fab demo
```

The parameters for the transaction rate (etc) can be adjusted by editing the `fabfile.py` file.

## Via Docker (NOT WORKING RELIABLY YET)

1. First start up the narwhal cluster via `docker-compose`

```
$ docker-compose -f docker-compose.yml up
```

2. Run the data seeder via the benchmark tooling. Note that the port provided is the starting port for which the workers will receive transactions. Seeder will autoincrement port for the number of nodes * workers provided in the parameters.

```
fab seed 7001
```

Parameters for transaction rate can be adjusted in `/benchmark/fabfile.py`
```
bench_params = {
        'faults': 0,
        'nodes': 4,
        'workers': 1,
        'rate': 50_000,
        'tx_size': 512,
        'duration': 20,
    }
```

3. Then run the `demo-client` 

```
cargo run --all-features --package demo --bin demo_client
```