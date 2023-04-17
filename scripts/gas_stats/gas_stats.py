from os import path
from glob import glob
from collections import defaultdict
from pathlib import Path

# Main holder of gas stats. It's a dictionary keyed off the transaction digest.
# Values are stats related to gas for the given transaction
gas_stats = defaultdict(dict)

# Parse all stats* files, create and populate gas_stats
def parse_files():
    files_path = path.join(Path.home(), "tmp/track_gas/stats*")
    stats_files = glob(files_path)
    for file_name in stats_files:
        file = open(file_name)
        while True:
            line = file.readline()
            if not line:
                break
            parse_line(line)

# parse a single line
def parse_line(line):
    # print(line)
    key_value = line.split(':', 1)
    gas_entry = gas_stats[key_value[0].strip()]
    process_value(gas_entry, key_value[1].strip())

# parse the values in a line for a given transaction digest
def process_value(gas_entry, line):
    idx = 0
    while idx < len(line) - 1:
        c = line[idx]
        if c.isspace() or c == ',':
            idx += 1
            continue
        (key, idx) = get_key(line, idx)
        # print("key: {} - idx: {}".format(key, idx))
        if key in ["packages", "modules", "functions"]:
            (value, idx) = get_values(line, idx)
        else:
            (value, idx) = get_value(line, idx)
        # print("value: {} - idx: {}".format(value, idx))
        gas_entry[key] = value

# get a key in a list of stats (key, value) pairs
def get_key(line, start):
    idx = start
    key = ""
    for idx in range(idx, len(line)):
        c = line[idx]
        if c.isidentifier():
            key += c
        else:
            break
    return key, idx

# get an integer value for a gas stat
def get_value(line, start):
    # import pdb; pdb.set_trace()
    idx = start
    value = ""
    for idx in range(idx, len(line)):
        c = line[idx]
        if c.isspace() or c == '=':
            continue
        if c.isnumeric():
            value += c
        else:
            break
    return int(value), idx

# get an array of key/value pairs for packages, module and functions in a programmable transaction
def get_values(line, start):
    # import pdb; pdb.set_trace()
    idx = start

    # read until '['
    while idx < len(line):
        c = line[idx]
        idx += 1
        if c.isspace() or c == '=':
            continue
        elif c == '[':
            break
        else:
            raise Exception("Unexpected character while look for '['")

    # read values in '[...]'
    key_value = {}
    while idx < len(line):
        c = line[idx]
        idx += 1
        if c == ']':
            break
        elif c.isspace() or c == ',':
            continue
        else:
            key = c
            while idx < len(line):
                c = line[idx]
                idx += 1
                if c == '=':
                    break
                key += c
            key = key.strip()
            (value, idx) = get_value(line, idx)
            key_value[key] = value

    return key_value, idx

# stats summaries
def check_transactions():
    genesis = 0
    consensus_commit_prologue = 0
    change_eopch = 0
    programmable_transaction = 0
    unknown = 0
    for values in gas_stats.values():
        if "genesis" in values:
            genesis += 1
        elif "consensus_commit_prologue" in values:
            consensus_commit_prologue += 1
        elif "change_epoch" in values:
            change_eopch += 1
        elif "programmable_transaction" in values:
            programmable_transaction += 1
        else:
            unknown += 1
            # raise Exception("unknown transaction type for {}".format(values))
    print(
        "genesis = {}, consensus_commit_prologue = {}, change_eopch = {}, programmable_transaction = {}, unknown = {}".format(
            genesis,
            consensus_commit_prologue,
            change_eopch,
            programmable_transaction,
            unknown,
        )
    )

def print_times():
    for digest, values in gas_stats.items():
        if "time" not in values:
            continue
        time = values["time"]
        if "genesis" in values:
            print("genesis, {}".format(time))
        elif "consensus_commit_prologue" in values:
            print("consensus_commit_prologue, {}".format(time))
        elif "change_epoch" in values:
            print("change_epoch, {}".format(time))
        elif "programmable_transaction" in values:
            print("programmable_transaction, {}".format(time))
        else:
            print("unknown, {}".format(time))
            # raise Exception("unknown transaction type for {} - {}".format(digest, values))


def print_programmable_transactions():
    print("digest, time, error, total_cost, computation_cost, storage_cost, storage_rebate, instructions, stack_height, stack_size, function count, functions, publishes, splits, merges, make_vecs, transfers")
    for digest, values in gas_stats.items():
        if "time" not in values:
            continue
        if "programmable_transaction" not in values:
            continue
        functions = ""
        if "functions" in values:
            for function in values["functions"]:
                functions += function + " - "
        total_cost = -1
        if "computation_cost" in values and "storage_cost" in values and "storage_rebate" in values:
            total_cost = values["computation_cost"] + values["storage_cost"] - values["storage_rebate"]
        print(
            "{}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}".format(
                digest,
                values["time"],
                values.get("error", -1),
                total_cost,
                values.get("computation_cost", -1),
                values.get("storage_cost", -1),
                values.get("storage_rebate", -1),
                values.get("instructions", -1),
                values.get("stack_height", -1),
                values.get("stack_size", -1),
                len(values.get("functions", {})),
                functions,
                values.get("publishes", -1),
                values.get("splits", -1),
                values.get("merges", -1),
                values.get("make_vecs", -1),
                values.get("transfers", -1),
            )
        )


# main...
def main():
    parse_files()
    # check_transactions()
    # print_times()
    print_programmable_transactions()

if __name__ == "__main__":
   main()
