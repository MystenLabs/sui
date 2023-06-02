Example invocations to run benchmark on local machine:

1. Server
```
target/debug/anemo-benchmark --port 5556
```

1. Client (look here for output)
```
target/debug/anemo-benchmark --port 5555 --addrs 127.0.0.1:5556 --requests-up 10 --requests-down 10 --size-up 5000 --size-down 5000
```