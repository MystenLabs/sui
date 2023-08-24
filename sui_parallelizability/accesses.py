# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

import pandas as pd

def print_val(val, sum_val, num_tx):
    frac = val / sum_val * 100.0
    per_tx = val / num_tx
    print(f'{val} - {frac:.4}% - {per_tx:.2}/tx')

df = pd.read_csv('./data/batch150_accesses.csv')

max_tx = len(df.index)

print(f'First {max_tx+1} txs:')

sum_imm = df['immutable'].sum()
sum_gas = df['gas'].sum()
sum_own = df['owned'].sum()
sum_cre = df['created'].sum()
sum_shr = df['shared_read'].sum()
sum_shw = df['shared_write'].sum()
sum_oth = df['other'].sum()
sum_total = sum_imm + sum_gas + sum_own + sum_cre + sum_shr + sum_shw + sum_oth

print_val(sum_imm, sum_total, max_tx)
print_val(sum_gas, sum_total, max_tx)
print_val(sum_own, sum_total, max_tx)
print_val(sum_cre, sum_total, max_tx)
print_val(sum_shr, sum_total, max_tx)
print_val(sum_shw, sum_total, max_tx)
print_val(sum_oth, sum_total, max_tx)
