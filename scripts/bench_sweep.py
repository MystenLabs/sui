from time import sleep
import matplotlib.pyplot as plt
import subprocess
import ast
from string import Template

cmd_template = Template(
    "../target/release/microbench_latency --period-us $period_us --chunk-size $chunk_size --num-chunks $num_chunks")

def get_avg_latency(period_us, chunk_size, num_chunks):
    cmd = cmd_template.substitute(
        period_us=period_us, chunk_size=chunk_size, num_chunks=num_chunks)
    print(cmd)
    process = subprocess.Popen(cmd.split(), stdout=subprocess.PIPE)
    output, error = process.communicate()

    resp = output.decode("utf-8")
    res = ast.literal_eval(resp)
    print(res)
    # Pick upper half at steady state
    res = res[len(res)//2:]
    return sum(res)/len(res)


def plot(vals):
    plt.title("Latency vs Throughput")
    plt.scatter(*zip(*vals))
    plt.ylabel("Latency (ms)")
    plt.xlabel("Throughput")
    plt.show()

lats = []
for i in range(10):
    chunk_size = 200 * (i+1)
    period_us = 10000
    num_chunks = 10
    thr = chunk_size*1000*1000/period_us
    avg_lat_ms = get_avg_latency(period_us, chunk_size, num_chunks)/1000
    lats.append((thr, avg_lat_ms))
    sleep(1)
plot(lats)
