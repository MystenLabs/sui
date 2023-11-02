# Copyright (c) The Diem Core Contributors
# Copyright (c) The Move Contributors
# SPDX-License-Identifier: Apache-2.0

alias coverage_update="pkill cargo; cargo run --release --bin move-trace-conversion -- -f $HOME/trace -u trace.mvcov -o trace.mvcov; rm -rf $HOME/trace"
alias coverage_summary="pkill cargo; cargo run --release --bin coverage-summaries -- -t trace.mvcov -s ../../../diem-move/diem-framework/DPN/releases/artifacts/current/modules"
function module_coverage() {
    pkill cargo; cargo run --release --bin source-coverage -- -t trace.mvcov -b "../../move-compiler/build/modules/$1.mv" -s "../../../diem-move/diem-framework/core/sources/$2.move" -o tmp;
    less tmp
}
