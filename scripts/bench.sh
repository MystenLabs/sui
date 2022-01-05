#!/bin/bash
# Copyright (c) Facebook, Inc. and its affiliates.
# SPDX-License-Identifier: Apache-2.0

num_accounts=4000
max_in_flight=1000
gas_objs_per_account=4
value_per_per_obj=200
committee_size=4
protocol=TCP

if [ "$1" != "" ]; then
	num_accounts=$1
fi
if [ "$2" != "" ]; then
	max_in_flight=$2
fi
if [ "$3" != "" ]; then
	committee_size=$3
fi
if [ "$4" != "" ]; then
	protocol=$4
fi
if [ "$5" != "" ]; then
	gas_objs_per_account=$5
fi
if [ "$6" != "" ]; then
	value_per_per_obj=$6
fi


# Distinguish local and aws tests.
if [ "$6" != "aws" ]; then 
	cd ../../target/release/
fi

# Clean up.
killall server || true
killall client || true
rm *.json || true
rm *.toml || true
rm -rf db* || true

# Create committee and server configs.
key_files=""
for (( i=1; i<=$committee_size; i++ ))
do
	key_files="$key_files server$i.json"
	mkdir ./db"$i"
	./server --server server"$i".json generate \
		--host 127.0.0.1 \
		--port 9"$i"00 \
		--database-path ./db"$i" \
		--protocol $protocol \
		>> committee.json 
done

# Create clients' accounts.
./client --committee committee.json --accounts accounts.json create-accounts --num $num_accounts \
--gas-objs-per-account $gas_objs_per_account --value-per-per-obj $value_per_per_obj initial_accounts.toml

# Run a authorities.
for (( I=1; I<=$committee_size; I++ ))
do
    ./server --server server"$I".json run --initial-accounts initial_accounts.toml --committee committee.json &
done

# Run the client benchmark.
sleep 2 # wait for servers to be ready before benchmark
./client --committee committee.json --accounts accounts.json benchmark --max-in-flight $max_in_flight
