# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
#
# Correctness-harness image for an in-cluster one-off Job.
# Build context = grpc-list-testing/ (parent dir), so the corpus is in scope.
# Generate stubs FIRST:  correctness/gen_stubs.sh   (bakes correctness/sui_pb)
#
#   docker build -f correctness/Dockerfile -t <registry>/ledger-correctness:<tag> .
FROM python:3.12-slim
RUN pip install --no-cache-dir grpcio protobuf
WORKDIR /harness
COPY correctness/ /harness/correctness/
COPY corpus.testnet.jsonl /harness/corpus.testnet.jsonl
ENTRYPOINT ["python", "/harness/correctness/harness.py"]
