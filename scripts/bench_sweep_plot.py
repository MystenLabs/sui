import matplotlib.pyplot as plt
import subprocess
import ast


cmd = "../target/release/bench_sweep --tx-start 1000 --tx-end 200000 --tx-step 5000 --batch-size 50"
process = subprocess.Popen(cmd.split(), stdout=subprocess.PIPE)
output, error = process.communicate()

resp = output.decode("utf-8")
res = ast.literal_eval(resp)
vals = [(v[0], v[1]) for v in res]

plt.scatter(*zip(*vals))
plt.xlabel("Number of transactions")
plt.ylabel("Throughput (tx/sec)")
plt.show()