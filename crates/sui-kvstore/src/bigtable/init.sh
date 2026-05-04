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

for table in checkpoints checkpoints_by_digest transactions objects epochs \
    watermark_alt protocol_configs packages packages_by_id \
    packages_by_checkpoint system_packages tx_seq_digest; do
  (
    set -x
    "${command[@]}" createtable $table
    "${command[@]}" createfamily $table sui
    "${command[@]}" setgcpolicy $table sui maxversions=1
  )
done
