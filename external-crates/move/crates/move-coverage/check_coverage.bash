#!/bin/bash

# Copyright (c) The Diem Core Contributors
# Copyright (c) The Move Contributors
# SPDX-License-Identifier: Apache-2.0

TRACE_PATH=$HOME/trace

[ ! -e  "$TRACE_PATH" ] || rm -f "$TRACE_PATH"

export MOVE_VM_TRACE=$TRACE_PATH

echo "Rebuilding stdlib..."
pushd ../../../diem-move/diem-framework || exit 1
cargo run
popd || exit 1

# TODO: add coverage for transactional tests

echo "---------------------------------------------------------------------------"
echo "Running e2e testsuite..."
echo "---------------------------------------------------------------------------"
pushd ../../e2e-testsuite || exit 1
cargo test -- --skip account_universe --skip fuzz_scripts
popd || exit 1

echo "---------------------------------------------------------------------------"
echo "Running Move testsuite..."
echo "---------------------------------------------------------------------------"
pushd ../../move-compiler/functional-tests/tests || exit 1
cargo test
popd || exit 1

echo "---------------------------------------------------------------------------"
echo "Building Move modules and source maps.."
echo "---------------------------------------------------------------------------"
pushd ../../move-compiler || exit 1
rm -rf build
cargo run --bin move-build -- ../../diem-move/diem-framework/core/sources -m
popd || exit 1

echo "---------------------------------------------------------------------------"
echo "Converting trace file..."
echo "---------------------------------------------------------------------------"
cargo run --bin move-trace-conversion -- -f "$TRACE_PATH" -o trace.mvcov

echo "---------------------------------------------------------------------------"
echo "Producing coverage summaries..."
echo "---------------------------------------------------------------------------"
cargo run --bin coverage-summaries -- -t trace.mvcov -s ../../../diem-move/diem-framework/DPN/releases/artifacts/current/modules

echo "==========================================================================="
echo "You can check source coverage for a module by running:"
echo "> cargo run --bin source-coverage -- -t trace.mvcov -b ../../move-compiler/build/modules/<LOOK_FOR_MODULE_HERE>.mv -s ../../../diem-move/diem-framework/core/modules/<SOURCE_MODULE>.move"
echo "---------------------------------------------------------------------------"
echo "You can can also get a finer-grained coverage summary for each function by running:"
echo "> cargo run --bin coverage-summaries -- -t trace.mvcov -s ../../../diem-move/diem-framework/DPN/releases/artifacts/current/stdlib.mv"
echo "==========================================================================="

unset MOVE_VM_TRACE

echo "DONE"
