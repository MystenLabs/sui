# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
INSTANCE_ID=${1:-sui}
command=(
  cbt
  -instance
  "$INSTANCE_ID"
)
if [[ -n $BIGTABLE_EMULATOR_HOST ]]; then
  command+=(-project emulator)
fi

for table in objects transactions checkpoints checkpoints_by_digest watermark; do
  (
    set -x
    "${command[@]}" createtable $table
    "${command[@]}" createfamily $table sui
    "${command[@]}" setgcpolicy $table sui maxversions=1
  )
done
"${command[@]}" setgcpolicy watermark sui maxage=2d
