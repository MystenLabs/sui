import re
from collections import Counter

# https://github.com/MystenLabs/sui/blob/instrument_tasks/crates/sui-core/src/authority.rs#L15
base_url = "https://github.com/MystenLabs/sui/blob/instrument_tasks/"

data = open("wake_trace.txt").read()
name_lines = re.findall("WAKE NAME ([0-9]+) (.+) FROM ([0-9]+)", data)

names = {"0": "-"}
relations = {}
drops = {}
spawns = {}
returns = {}
node_names = {}
node_names_num = 1
cnt = {}
cycles = {}

for (id, name, from_id) in name_lines:
    names[id] = name
    node_names[name] = f"n{node_names_num}"
    node_names_num += 1

    try:
        spawns[name] += 1
    except:
        spawns[name] = 1

    try:
        relations[("spawn", name, names[from_id])] += 1
    except:
        relations[("spawn", name, names[from_id])] = 1

wake_lines = re.findall("WAKE WAKE ([0-9]+) from ([0-9]+)", data)

for (idt, idf) in wake_lines:
    try:
        relations[("wake", names[idt], names[idf])] += 1
    except:
        relations[("wake", names[idt], names[idf])] = 1

retn_lines = re.findall("WAKE RETN ([0-9]+) from ([0-9]+)", data)

for (idt, idf) in retn_lines:

    name = names[idf]

    try:
        returns[name] += 1
    except:
        returns[name] = 1

    try:
        relations[("return", names[idt], names[idf])] += 1
    except:
        relations[("return", names[idt], names[idf])] = 1

drop_lines = re.findall("WAKE DROP ([0-9]+) CYCLES ([0-9]+)", data)
total_inst = 0

for nid, inst in drop_lines:
    name = names[nid]
    total_inst += int(inst)

    try:
        drops[name] += 1
        cycles[name] += int(inst)
    except:
        drops[name] = 1
        cycles[name] = int(inst)

# ----

urls = {}
for (name, nid) in node_names.items():
    [base, ref] = name.split(":")
    try:
        ref = int(ref.strip())
        url = base_url + base + "#L" + str(ref)
    except:
        url = base_url+base
    urls[name] = url
    
for ((w, n, t), v) in relations.items():
    if t != "-":
        cnt[n] = cnt.get(n,0)+1
        cnt[t] = cnt.get(t,0)+1

node_str = ""
for name, nid in node_names.items():
    # if cnt.get(name,0) > 0:
    num = relations.get(("spawn", name, "-"), 0)
    num_str = f" ({num})" if num > 0 else ""

    inflight = int(spawns.get(name,0))- int(drops.get(name,0))
    spawn_drop = f" in-flight:{inflight}" if inflight > num else ""

    color = 'color="red",' if spawn_drop != "" else ""

    cancelled = int(drops.get(name,0))- int(returns.get(name,0)) 
    cancel_drop = f" cancel:{cancelled}" if cancelled > 0 else ""

    cpu_pc = cycles.get(name,0)/total_inst
    cpu = f" cpu:{cpu_pc:.2%}" if cpu_pc > 0.005 else ""
    fill= ',style=filled, fillcolor="cornsilk"' if cpu_pc > 0.005 else ""

    node_str += f'{nid} [{color}label="{name}{spawn_drop}{cancel_drop}{num_str}{cpu}", shape=box, href="{urls[name]}"{fill}];\n'

style = {
    "spawn" : "bold",
    "wake": "solid",
    "return" : "dashed",
}

for ((w, nto, nftom), v) in relations.items():
    if nftom != "-" and nto!="-":
        # print(w, node_names[n], node_names[t], v)
        node_str += f'{node_names[nftom]} -> {node_names[nto]} [style={style[w]}, label="{v}"];\n'

print("""digraph {
     overlap = false;
    splines = true;
       """ + node_str+"}")