# Copyright (c) Facebook, Inc. and its affiliates.
    # Run as:
#  $ grep CERT: node-1.log > xx.txt
#  $ python blockexplore.py xx.txt > g.dot
#  $ dot -Kneato -n -Tsvg -o sample.svg g.dot

import re
import sys

data = open(sys.argv[1], 'r').read()
data = re.findall(
    "\[([0-9]{4})-([0-9]{2})-([0-9]{2})T([0-9]{2}):([0-9]{2}):([0-9]{2})\.([0-9]{3})Z INFO  dag_core::primary] CERT: \(([^,]*),([^\)]*)\) Deps: \{([^}]*)\} Txs: \{([^}]*)\}",
    data)

class Seq:
    def __init__(self):
        self.cnt = 0
        self.d = {}

    def lookup(self, item):
        if item not in self.d:
            self.d[item] = self.cnt
            self.cnt += 1
        return self.d[item]


seq = Seq()
inversemapx = {}
mapx = {}
listx = []
xmin = None
for (_, _, _, hx, mx, sx, ms, senderid, xround, deps, txs) in data:
    mins = int(hx) * 60 + int(mx)
    secs = 60 * mins  + int(sx)
    millis = 1000 * secs + int(ms)

    if xmin is None:
        xmin = millis

    xtime = millis - xmin
    sender = seq.lookup(senderid)
    xround = int(xround)
    others = list(map(seq.lookup, re.findall("\(([^,]+), [^\)]+\)", deps)))
    volume = re.findall("\(([^,]+), [^\)]+\)", txs)
    # print(millis)
    #if (sender, xround) in mapx:
    #    print((sender, xround))

    mapx[(sender, xround)] = (xtime, sender, xround, others, volume)
    listx += [(xtime, sender, xround, others, volume)]

    for parent in others:
        inversemapx[(parent, xround-1)] = None

print("digraph \"blocks\" {")
print("  graph [outputorder=edgesfirst];")
stored = {}
txs = {}
for (xtime, sender, xround, others, volume) in listx:
    # (xtime, sender, xround, others, volume) = mapx[(sender, xround)]
    if sender not in txs:
        txs[sender] = set()

    prev_txs = txs[sender]
    tx_add = set(volume) - prev_txs
    tx_commit = prev_txs - set(volume)
    label = f'+{len(tx_add)}-{len(tx_commit)}'
    txs[sender] = set(volume)

    shape="circle"
    add = ""
    col = 'white'
    if xround % 2 == 1:
        col = "gainsboro"
    if (sender, xround) not in inversemapx:
        col = 'red'
    if len(volume) > 0:
        shape="box"
        add = f' ({len(volume)})'
    if (sender, xround) not in stored:
        print(f'   n{sender}r{xround} [label="{str(xround) + add}" pos="{100*sender},{xtime}!" shape={shape} style=filled fillcolor="{col}"];')
        stored[(sender, xround)] = 0
    else:
        stored[(sender, xround)] += 1
        print(f'   n{sender}r{xround}v{stored[(sender, xround)]} [label="{str(xround) + add}" pos="{100*sender},{xtime}!" shape={shape} style=filled fillcolor="mistyrose"];')


done = {}
for (xtime, sender, xround, others, volume) in listx:
    if (sender, xround) in done:
        continue
    done[(sender, xround)] = 1
    # (xtime, sender, xround, others, volume) = mapx[(sender, xround)]
    for parent in others:
        if (parent, xround-1) in mapx:
            print(f'  n{parent}r{xround-1} -> n{sender}r{xround};')




print("}")
