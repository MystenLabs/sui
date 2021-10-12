#!/bin/bash
# Copyright (c) Facebook, Inc. and its affiliates.
# SPDX-License-Identifier: Apache-2.0

num_shards=15
num_accounts=500000
max_in_flight=700
committee_size=4
protocol=UDP

if [ "$1" != "" ]; then
	num_shards=$1
fi
if [ "$2" != "" ]; then
	num_accounts=$2
fi
if [ "$3" != "" ]; then
	max_in_flight=$3
fi
if [ "$4" != "" ]; then
	committee_size=$4
fi
if [ "$5" != "" ]; then
	protocol=$5
fi

# Distinguish local and aws tests.
if [ "$6" != "aws" ]; then 
	cd ../../target/release/
fi

# Clean up.
killall server || true
killall client || true
rm *.json || true

# Create committee and server configs.
key_files=""
for (( i=1; i<=$committee_size; i++ ))
do
	key_files="$key_files server-$i.json"
	./server --server server-"$i".json generate \
		--host 127.0.0.1 \
		--port 9500 \
		--shards $num_shards \
		--protocol $protocol \
		>> committee.json 
done

# Create clients' accounts.
./client --committee committee.json --accounts accounts.json create_accounts $num_accounts >> initial_accounts.json

# Run a single authority (with multiple shards).
for (( i=0; i<$num_shards; i++ ))
do
	./server --server server-1.json run \
		--initial_accounts initial_accounts.json \
		--initial_balance 100 \
		--committee committee.json \
		--shard $i &
done

# Run the client benchmark.
sleep 1 # wait for server to be ready before benchmark
read -r line < committee.json
echo "$line" > committee-single.json
./client --committee committee-single.json --accounts accounts.json benchmark \
	--server_configs $key_files \
	--max_in_flight $max_in_flight
	
