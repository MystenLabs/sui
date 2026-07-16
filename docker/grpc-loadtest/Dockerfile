# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
#
# k6 load-test image for the stable-v2 gRPC List APIs
# (ListTransactions / ListEvents / ListCheckpoints).
#
# Unlike the other images under docker/ (which cargo-build a Rust binary), this
# bakes a k6 script + protos + generated request lists. It is built by meta-svc
# with **context = repo root** (NOT --context-sub-path, which yields an empty
# context on meta-svc) and the Dockerfile passed via --dockerfile. So every COPY
# source is REPO-ROOT-RELATIVE. Inputs are staged (committed) by build.sh under
# grpc-list-testing/loadtest-context/ -- a path the repo-root .dockerignore does
# NOT exclude (unlike docker/).
#
# Stock grafana/k6 already ships the k6/net/grpc module (server-streaming +
# grpc.Stream) proven sufficient in the streaming spike (testing_plan.md §2.5) --
# no xk6 extension needed. Pinned to 0.54.0: the first release where stream
# params accept discardResponseMessage (load.k6.js DISCARD=1), which skips k6's
# per-message protojson/JSON/goja conversion so capacity runs measure the server
# rather than the client's decode stack.
FROM grafana/k6:0.54.0

COPY grpc-list-testing/loadtest-context/load.k6.js /scripts/load.k6.js
COPY grpc-list-testing/loadtest-context/proto /proto
COPY grpc-list-testing/loadtest-context/data/ /data/

ENTRYPOINT ["k6"]
CMD ["run", "/scripts/load.k6.js"]
