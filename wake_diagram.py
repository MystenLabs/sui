import re
from collections import Counter

data = open("wake_trace.txt").read()
name_lines = re.findall("WAKE NAME ([0-9]+) (.+) FROM ([0-9]+)", data)

names = {"0": "-"}
relations = {}
node_names = {}
node_names_num = 1
cnt = {}

for (id, name, from_id) in name_lines:
    names[id] = name
    node_names[name] = f"n{node_names_num}"
    node_names_num += 1

    try:
        relations[("spawn", name, names[from_id])] += 1
    except:
        relations[("spawn", name, names[from_id])] = 1

wake_lines = re.findall("WAKE ([0-9]+) from ([0-9]+)", data)

for (idt, idf) in wake_lines:
    try:
        relations[("wake", names[idt], names[idf])] += 1
    except:
        relations[("wake", names[idt], names[idf])] = 1

for ((w, n, t), v) in relations.items():
    if t != "-":
        cnt[n] = cnt.get(n,0)+1
        cnt[t] = cnt.get(t,0)+1

node_str = ""
for name, nid in node_names.items():
    if cnt.get(name,0) > 0:
        num = relations.get(("spawn", name, "-"), 0)
        node_str += f'{nid} [label="{name} ({num})", shape=box];\n'

for ((w, nto, nftom), v) in relations.items():
    if nftom != "-" and nto!="-":
        # print(w, node_names[n], node_names[t], v)
        node_str += f'{node_names[nftom]} -> {node_names[nto]} [penwidth={3.0 if w == "spawn" else 1.0}, label="{v}"];\n'

print("digraph {\noverlap=scale;\n " + node_str+"}")