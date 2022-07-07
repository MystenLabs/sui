#!/usr/bin/env python3
"""Create a docker-compose.yaml file to stdout for --num # of nodes
"""

import sys
import argparse

HEADER = """---
version: "3"

volumes:
    prometheus_data: {}
    grafana_data: {}

networks:
  default:

services:

  prometheus:
    image: prom/prometheus:v2.36.0
    container_name: prometheus
    volumes:
      - ./prometheus:/etc/prometheus
      - prometheus_data:/prometheus
    command:
      - '--config.file=/etc/prometheus/prometheus.yml'
      - '--storage.tsdb.path=/prometheus'
      - '--web.console.libraries=/etc/prometheus/console_libraries'
      - '--web.console.templates=/etc/prometheus/consoles'
      - '--storage.tsdb.retention.time=200h'
      - '--web.enable-lifecycle'
    restart: unless-stopped
    expose:
      - 9090
    labels:
      org.label-schema.group: "monitoring"

  grafana:
    image: grafana/grafana:8.5.4
    container_name: grafana
    volumes:
      - grafana_data:/var/lib/grafana
      - ./grafana/provisioning/dashboards:/etc/grafana/provisioning/dashboards
      - ./grafana/provisioning/datasources:/etc/grafana/provisioning/datasources
    environment:
      - GF_SECURITY_ADMIN_USER=${ADMIN_USER:-admin}
      - GF_SECURITY_ADMIN_PASSWORD=${ADMIN_PASSWORD:-admin}
      - GF_USERS_ALLOW_SIGN_UP=false
    restart: unless-stopped
    expose:
      - 3000
    ports:
      - 3000:3000
    labels:
      org.label-schema.group: "monitoring"

  nodeexporter:
    image: prom/node-exporter:v1.3.1
    container_name: nodeexporter
    volumes:
      - /proc:/host/proc:ro
      - /sys:/host/sys:ro
      - /:/rootfs:ro
    command:
      - '--path.procfs=/host/proc'
      - '--path.rootfs=/rootfs'
      - '--path.sysfs=/host/sys'
      - '--collector.filesystem.mount-points-exclude=^/(sys|proc|dev|host|etc)($$|/)'
    restart: unless-stopped
    expose:
      - 9100
    labels:
      org.label-schema.group: "monitoring"

  # Does not work on MacOS, only Linux.
  # cadvisor:
  #   image: gcr.io/cadvisor/cadvisor:v0.44.0
  #   container_name: cadvisor
  #   privileged: true
  #   devices:
  #     - /dev/kmsg:/dev/kmsg
  #   volumes:
  #     - /:/rootfs:ro
  #     - /var/run:/var/run:ro
  #     - /sys:/sys:ro
  #     - /var/lib/docker:/var/lib/docker:ro
  #     - /cgroup:/cgroup:ro
  #   restart: unless-stopped
  #   expose:
  #     - 8080
  #   labels:
  #     org.label-schema.group: "monitoring"

  read:
    image: grafana/loki:2.5.0
    command: "-config.file=/etc/loki/config.yaml -target=read"
    ports:
      - 3101:3100
      - 7946
      - 9095
    volumes:
      - ./loki-config.yaml:/etc/loki/config.yaml
    depends_on:
      - minio
    networks: &loki-dns
      default:
        aliases:
          - loki

  write:
    image: grafana/loki:2.5.0
    command: "-config.file=/etc/loki/config.yaml -target=write"
    ports:
      - 3102:3100
      - 7946
      - 9095
    volumes:
      - ./loki-config.yaml:/etc/loki/config.yaml
    depends_on:
      - minio
    networks:
      <<: *loki-dns

  promtail:
    image: grafana/promtail:2.5.0
    volumes:
      - ./promtail-local-config.yaml:/etc/promtail/config.yaml:ro
      - /var/run/docker.sock:/var/run/docker.sock
      - /var/lib/docker/containers:/var/lib/docker/containers
      - ./:/validators:ro
    command: -config.file=/etc/promtail/config.yaml
    depends_on:
      - gateway

  minio:
    image: minio/minio
    entrypoint:
      - sh
      - -euc
      - |
        mkdir -p /data/loki-data /data/loki-ruler && \
        minio server /data
    environment:
      - MINIO_ACCESS_KEY=loki
      - MINIO_SECRET_KEY=supersecret
      - MINIO_PROMETHEUS_AUTH_TYPE=public
      - MINIO_UPDATE=off
    ports:
      - 9000
    volumes:
      - ./data/minio:/data

  gateway:
    image: nginx:latest
    depends_on:
      - read
    entrypoint:
      - sh
      - -euc
      - |
        cat <<EOF > /etc/nginx/nginx.conf
        user  nginx;
        worker_processes  5;  ## Default: 1

        events {
          worker_connections   1000;
        }

        http {
          resolver 127.0.0.11;

          server {
            listen             3100;

            location = / {
              return 200 'OK';
              auth_basic off;
            }

            location = /api/prom/push {
              proxy_pass       http://write:3100\$$request_uri;
            }

            location = /api/prom/tail {
              proxy_pass       http://read:3100\$$request_uri;
              proxy_set_header Upgrade \$$http_upgrade;
              proxy_set_header Connection "upgrade";
            }

            location ~ /api/prom/.* {
              proxy_pass       http://read:3100\$$request_uri;
            }

            location = /loki/api/v1/push {
              proxy_pass       http://write:3100\$$request_uri;
            }

            location = /loki/api/v1/tail {
              proxy_pass       http://read:3100\$$request_uri;
              proxy_set_header Upgrade \$$http_upgrade;
              proxy_set_header Connection "upgrade";
            }

            location ~ /loki/api/.* {
              proxy_pass       http://read:3100\$$request_uri;
            }
          }
        }
        EOF
        /docker-entrypoint.sh nginx -g "daemon off;"
    ports:
      - "3100:3100"


"""

FOOTER = """...
"""

def main():
    "mainly main."
    parser = argparse.ArgumentParser(description="docker compose config generator")

    parser.add_argument("-n", default=4, type=int, help="number of primary+worker instances")
    parser.add_argument("-t", default="node.template", help="node template file")
    args = parser.parse_args()

    templ = open(args.t).read()

    print(HEADER)

    for i in range(args.n):
        tmp = "{:02d}".format(i)
        print(templ.format(counter=tmp, num=args.n))

    print(FOOTER)

if __name__ == '__main__':
    sys.exit(main())
