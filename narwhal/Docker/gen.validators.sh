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
    "sync_retry_nodes": 3,
    "prometheus_metrics": {
        "socket_addr": "0.0.0.0:8010"
    }
}
EOF

./scripts/gen.committee.py -n ${num} -d ${target} > ${target}/committee.json

cp -r templates/{grafana,prometheus} ${target}/

# add the primary and worker nodes to the prometheus.yaml scrape configs.
t=$(($num - 1))
for i in $(seq -f %02g 0 ${t})
do
    scrape="primary_${i}:8010"
    cat >> ${target}/prometheus/prometheus.yml <<EOF

  - job_name: 'primary_${i}'
    scrape_interval: 10s
    static_configs:
      - targets: ['${scrape}']
EOF
done
