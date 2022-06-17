#!/usr/bin/env bash
set -e

# number of primary+worker instances to start.
num=$1

if [ -z "${num}" ]; then
    echo usage: $0 number_of_instances
    exit 1
fi

if [ ! -x ../target/release/node ]; then
    echo no narwhal node command found.
    exit 1
fi

node=../target/release/node

target=validators-${num}
mkdir -p $target

./scripts/gen.compose.py -n ${num} -t templates/node.template > ${target}/docker-compose.yaml

t=$(($num - 1))
for i in $(seq -f %02g 0 ${t})
do
    val=${target}/validator-${i}
    mkdir -p ${val}/{db-primary,db-worker-0,logs}
    ${node} generate_keys --filename ${val}/key.json
done

cat > ${target}/parameters.json <<EOF
{
    "batch_size": 500000,
    "block_synchronizer": {
        "certificates_synchronize_timeout": "2_000ms",
        "handler_certificate_deliver_timeout": "2_000ms",
        "payload_availability_timeout": "2_000ms",
        "payload_synchronize_timeout": "2_000ms"
    },
    "consensus_api_grpc": {
        "get_collections_timeout": "5_000ms",
        "remove_collections_timeout": "5_000ms",
        "socket_addr": "/ip4/0.0.0.0/tcp/8000/http"
    },
    "gc_depth": 50,
    "header_size": 1000,
    "max_batch_delay": "200ms",
    "max_concurrent_requests": 500000,
    "max_header_delay": "2000ms",
    "sync_retry_delay": "10_000ms",
    "sync_retry_nodes": 3
}
EOF

./scripts/gen.committee.py -n ${num} -d ${target} > ${target}/committee.json

cp -r templates/{grafana,prometheus} ${target}/
