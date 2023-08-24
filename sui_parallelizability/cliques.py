# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

import networkx as nx

cur_batch = None
graph = None

with open('./data/batch150_graphs.txt') as f:
    for line in f:
        tokens = line.split()
        if tokens[0] == 'batch':
            if graph is not None:
                print(nx.max_weight_clique(graph)[1])
            cur_batch = int(tokens[1])
            graph = nx.Graph()
        if tokens[0] == 'n':
            graph.add_node(int(tokens[1]), weight=int(tokens[2]))
        if tokens[0] == 'e':
            graph.add_edge(int(tokens[1]), int(tokens[2]))

print(nx.max_weight_clique(graph)[1])
