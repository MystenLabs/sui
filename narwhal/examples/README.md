### How to run demo client

1. First start up the narwhal cluster via `docker-compose`

```
$ docker-compose -f docker-compose.yml up
```

2. Run the data seeder via the benchmark tooling. 

```
fab seed $(pwd)/Docker/validators/committee.json
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
cargo run --package demo --bin demo_client
```