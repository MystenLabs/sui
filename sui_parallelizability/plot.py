# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

import matplotlib.pyplot as plt
import pandas as pd
import seaborn as sns

#sns.set_style('whitegrid')

#max_cliques = []
#with open('./data/max_cliques.txt') as f:
#    for line in f:
#        max_cliques.append(int(line))
#max_cliques = max_cliques[:57727]

df = pd.read_csv('./data/scheduling.csv')
#df = df[df['batch'] < 57727]
#df['max_clique'] = max_cliques
df['day'] = df['first_timestamp'] // (1000 * 60 * 60 * 24) - 19480
df['schedule_speedup'] = df['total_gas'] / df['sequential_gas']
df['cc_speedup'] = df['total_gas'] / df['max_cc']
#df['clique_speedup'] = df['total_gas'] / df['max_clique']

sns.lineplot(df, x='epoch', y='schedule_speedup', errorbar=('ci', 99))
sns.lineplot(df, x='epoch', y='cc_speedup', errorbar=('ci', 99))
#sns.lineplot(df, x='epoch', y='clique_speedup', errorbar=('ci', 99))
plt.axvline(x=20, color='r', ls=':', label='mainnet launch')
plt.yscale('log')
plt.ylabel('speedup')
plt.legend(['list scheduling', '99% CI', 'max CC', '99% CI', 'mainnet launch'])
#plt.legend(['list scheduling', '99% CI', 'max CC', '99% CI', 'max clique', '99% CI', 'mainnet launch'])
plt.tight_layout()
plt.show()

df_new = df.value_counts(['epoch'], sort=False).to_frame('count')
sns.lineplot(df_new, x='epoch', y='count', errorbar=None)
plt.axvline(x=20, color='r', ls=':', label='mainnet launch')
plt.yscale('log')
plt.ylabel('num batches (150 txs each)')
plt.legend(['num tx batches', 'mainnet launch'])
plt.tight_layout()
plt.show()
