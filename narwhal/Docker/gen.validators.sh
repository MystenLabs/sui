#!/usr/bin/env bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
set -e

# number of primary instances to start.
num_primary=$1

# number of worker instances per primary to start.
num_worker=$2

if [ -z "${num_primary}" ]; then
    echo usage: $0 number_of_instances
    exit 1
fi

if [ ! -x ../target/release/node ]; then
    echo no narwhal node command found.
    exit 1
fi

node=../target/release/node

target=validators-${num_primary}
mkdir -p $target

./scripts/gen.compose.py -np ${num_primary} -t templates/node.template > ${target}/docker-compose.yaml

# loki config
cat > ${target}/loki-config.yaml <<EOF
---
server:
  http_listen_port: 3100
memberlist:
  join_members:
    - loki:7946
schema_config:
  configs:
    - from: 2021-08-01
      store: boltdb-shipper
      object_store: s3
      schema: v11
      index:
        prefix: index_
        period: 24h
common:
  path_prefix: /loki
  replication_factor: 1
  storage:
    s3:
      endpoint: minio:9000
      insecure: true
      bucketnames: "loki-data"
      access_key_id: loki
      secret_access_key: supersecret
      s3forcepathstyle: true
  ring:
    kvstore:
      store: memberlist
ruler:
  storage:
    s3:
      bucketnames: loki-ruler
EOF

cat > ${target}/promtail-local-config.yaml <<EOF
---
server:
  http_listen_port: 9080
  grpc_listen_port: 0

positions:
  filename: /tmp/positions.yaml

clients:
  - url: http://gateway:3100/loki/api/v1/push
    tenant_id: tenant1

scrape_configs:
  - job_name: docker_log_scrape
    docker_sd_configs:
      - host: unix:///var/run/docker.sock
        refresh_interval: 5s
    relabel_configs:
      - source_labels: ['__meta_docker_container_name']
        regex: '/(.*)'
        target_label: 'container'

  - job_name: validators
    static_configs:
      - targets:
          - localhost
        labels:
          job: servicelogs
          __path__: /validators/validator-*/logs/log-*.txt
EOF

t=$(($num_primary - 1))
for i in $(seq -f %02g 0 ${t})
do
    val=${target}/validator-${i}
    mkdir -p ${val}/{db-primary,db-worker-0,logs}
    ${node} generate_keys --filename ${val}/key.json
    ${node} generate_network_keys --filename ${val}/network-key.json
done

cp validators/parameters.json ${target}/parameters.json

./scripts/gen.committee.py -n ${num_primary} -d ${target} > ${target}/committee.json
./scripts/gen.workers.py -np ${num_primary} -nw ${num_worker} -d ${target} > ${target}/workers.json

cp -r templates/{grafana,prometheus} ${target}/

# add the primary and worker nodes to the prometheus.yaml scrape configs.
t=$(($num_primary - 1))
for i in $(seq -f %02g 0 ${t})
do
    scrape_primary="primary_${i}:8010"
    scrape_worker="worker_${i}:8010"
    cat >> ${target}/prometheus/prometheus.yml <<EOF

  - job_name: 'primary_${i}'
    scrape_interval: 10s
    static_configs:
      - targets: ['${scrape_primary}']

  - job_name: 'worker_${i}'
    scrape_interval: 10s
    static_configs:
      - targets: ['${scrape_worker}']
EOF
done
