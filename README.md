<a href="https://novi.com/">
	<img width="100" src=".assets/novi.png" alt="Novi Logo" />
</a>

<hr/>

# FastPay

[![Build Status](https://github.com/novifinancial/fastpay/actions/workflows/rust.yml/badge.svg)](https://github.com/novifinancial/fastpay/actions/workflows/rust.yml)
[![License](https://img.shields.io/badge/license-Apache-green.svg)](LICENSE.md)

This repository is dedicated to sharing material related to the FastPay protocol, developed at Novi Financial (formerly Calibra). Software is provided for research-purpose only and is not meant to be used in production.

## Summary

FastPay allows a set of distributed authorities, some of which are Byzantine, to maintain a high-integrity and availability settlement system for pre-funded payments. It can be used to settle payments in a native unit of value (crypto-currency), or as a financial side-infrastructure to support retail payments in fiat currencies. FastPay is based on Byzantine Consistent Broadcast as its core primitive, foregoing the expenses of full atomic commit channels (consensus). The resulting system has low-latency for both confirmation and payment finality. Remarkably, each authority can be sharded across many machines to allow unbounded horizontal scalability. Our experiments demonstrate intra-continental confirmation latency of less than 100ms, making FastPay applicable to point of sale payments. In laboratory environments, we achieve over 80,000 transactions per second with 20 authorities---surpassing the requirements of current retail card payment networks, while significantly increasing their robustness.

## Quickstart with FastPay Prototype

```bash
cargo build --release
cd target/release
rm -f *.json

# Create configuration files for 4 authorities with 4 shards each.
# * Private server states are stored in `server*.json`.
# * `committee.json` is the public description of the FastPay committee.
for I in 1 2 3 4
do
    ./server --server server"$I".json generate --host 127.0.0.1 --port 9"$I"00 --shards 4 >> committee.json
done

# Create configuration files for 1000 user accounts.
# * Private account states are stored in one local wallet `accounts.json`.
# * `initial_accounts.json` is used to mint initial balances on the server side.
./client --committee committee.json --accounts accounts.json create_accounts 1000 --initial_funding 100 >> initial_accounts.json

# Start servers
for I in 1 2 3 4
do
    for J in $(seq 0 3)
    do
        ./server --server server"$I".json run --shard "$J" --initial_accounts initial_accounts.json --initial_balance 100 --committee committee.json &
    done
done

# Query (locally cached) balance for first and last user account
ACCOUNT1="`head -n 1 initial_accounts.json`"
ACCOUNT2="`tail -n -1 initial_accounts.json`"
./client --committee committee.json --accounts accounts.json query_balance "$ACCOUNT1"
./client --committee committee.json --accounts accounts.json query_balance "$ACCOUNT2"

# Transfer 10 units
./client --committee committee.json --accounts accounts.json transfer 10 --from "$ACCOUNT1" --to "$ACCOUNT2"

# Query balances again
./client --committee committee.json --accounts accounts.json query_balance "$ACCOUNT1"
./client --committee committee.json --accounts accounts.json query_balance "$ACCOUNT2"

# Launch local benchmark using all user accounts
./client --committee committee.json --accounts accounts.json benchmark

# Inspect state of first account
grep "$ACCOUNT1" accounts.json

# Kill servers
kill %1 %2 %3 %4 %5 %6 %7 %8 %9 %10 %11 %12 %13 %14 %15 %16

cd ../..
```

## References

* [FastPay: High-Performance Byzantine Fault Tolerant Settlement](https://arxiv.org/abs/2003.11506)

## Contributing

Read our [Contributing guide](https://developers.libra.org/docs/community/contributing).

## License

The content of this repository is licensed as [Apache 2.0](https://github.com/novifinancial/research/blob/master/LICENSE)
