# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
#
# k6 load-test image for the v2alpha gRPC List APIs
# (ListTransactions / ListEvents / ListCheckpoints).
#
# Unlike the other images under docker/ (which cargo-build a Rust binary with the
# repo root as context), this bakes a k6 script + protos + a generated request
# list. Its build context is a STAGED dir assembled by ./build.sh (the request
# list and protos are generated / vendored, not committed), so it is built via
# build.sh rather than `docker build <repo-root>`.
#
# Stock grafana/k6 already ships the k6/net/grpc module (server-streaming +
# grpc.Stream) proven sufficient in the streaming spike (testing_plan.md §2.5) --
# no xk6 extension needed.
#
# Expected context layout (build.sh produces it):
#   load.k6.js          the load script
#   proto/              merged proto root: sui/rpc/v2alpha/*, sui/rpc/v2/*, google/*
#   data/load.jsonl     the generated hot-key-spread request list (gen_load.py)
FROM grafana/k6:0.49.0

COPY load.k6.js /scripts/load.k6.js
COPY proto /proto
COPY data/load.jsonl /data/load.jsonl

ENTRYPOINT ["k6"]
CMD ["run", "/scripts/load.k6.js"]
