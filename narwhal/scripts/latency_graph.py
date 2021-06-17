import re
import statistics

# Parse the log file to digest related events

tx_block_include = {}
tx_block_commit  = {}
xround = None
round_time = None

round_delay_list = []

c = 0
for line in open("node-0.log"):
    c += 1

    # Update the round info
    m = re.search("Moving to round (.+) at time (.+)", line)
    if m:
        xround = int(m.group(1))
        new_round_time = int(m.group(2))

        if round_time:
            round_delay_list += [ new_round_time - round_time ]

        round_time = new_round_time

    # Detect own blocks
    m = re.search("Making header with txs digest (.+) at make time (.+)", line)
    if m:
        block = m.group(1)
        time  = int(m.group(2))

        if block not in tx_block_include:
            tx_block_include[block] = (time, xround)

    # Detect commit
    m = re.search("Commit digest (.+) at commit time (.+)", line)
    if m:
        block = m.group(1)
        time  = int(m.group(2))

        if block in tx_block_include:
            if block not in tx_block_commit:
                tx_block_commit[block] = (time, xround)



for block in tx_block_commit:
    in_time, in_round = tx_block_include[block]
    ct_time, ct_round = tx_block_commit[block]
    print(f"{block} {ct_round - in_round}rounds {ct_time-in_time}ms")

print(f"{statistics.mean(round_delay_list):2.2f}ms per round")
print(f"{c} lines")
