#!/bin/bash
set -x

cargo build --release
cd target/release
rm -f *.json *.toml
rm -rf db*

# Create DB dirs and configuration files for 4 authorities.
# * Private server states are stored in `server*.json`.
# * `committee.json` is the public description of the FastPay committee.
for I in 1 2 3 4
do
    mkdir ./db"$I"
    ./server --server server"$I".json generate --host 127.0.0.1 --port 9"$I"00 --database-path ./db"$I" >> committee.json
done

# Create configuration files for 100 user accounts, with 10 gas objects per account and 2000000 value each.
# * Private account states are stored in one local wallet `accounts.json`.
# * `initial_accounts.toml` is used to mint the corresponding initially randomly generated (for now) objects at startup on the server side.
./client --committee committee.json --accounts accounts.json create-accounts --num 100 \
--gas-objs-per-account 10 --value-per-per-obj 2000000 initial_accounts.toml
# Start servers
for I in 1 2 3 4
do
    ./server --server server"$I".json run --initial-accounts initial_accounts.toml --committee committee.json &
done

# Sleep for 5 seconds (for server to startup)
sleep 5

# Query account addresses
./client --committee committee.json --accounts accounts.json query-accounts-addrs

# Query (locally cached) object info for first and last user account
ACCOUNT1=`./client --committee committee.json --accounts accounts.json query-accounts-addrs | head -n 1`
ACCOUNT2=`./client --committee committee.json --accounts accounts.json query-accounts-addrs | tail -n -1`
./client --committee committee.json --accounts accounts.json query-objects --address "$ACCOUNT1"
./client --committee committee.json --accounts accounts.json query-objects --address "$ACCOUNT2"

# Get the first ObjectId for Account1
ACCOUNT1_OBJECT1=`./client --committee committee.json --accounts accounts.json query-objects --address "$ACCOUNT1" | head -n 1 |  awk -F: '{ print $1 }'`
# Pick the last item as gas object for Account1
ACCOUNT1_GAS_OBJECT=`./client --committee committee.json --accounts accounts.json query-objects --address "$ACCOUNT1" | tail -n -1 |  awk -F: '{ print $1 }'`
# Transfer object by ObjectID
./client --committee committee.json --accounts accounts.json transfer "$ACCOUNT1_OBJECT1" "$ACCOUNT1_GAS_OBJECT" --to "$ACCOUNT2" --from "$ACCOUNT1"
# Query objects again
./client --committee committee.json --accounts accounts.json query-objects --address "$ACCOUNT1"
./client --committee committee.json --accounts accounts.json query-objects --address "$ACCOUNT2"


# Kill servers
kill %1 %2 %3 %4 %5 %6 %7 %8 %9 %10 %11 %12 %13 %14 %15 %16

cd ../..

set +x