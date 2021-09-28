import matplotlib.pyplot as plt
import matplotlib.ticker as ticker


@ticker.FuncFormatter
def major_formatter(x, pos):
    return f"{x/1000:0.0f}k"


def major_formatter_sec(x, pos):
    return f"{x/1000:0.1f}"


plt.figure(figsize=[5, 3])

# Baseline HS
name = 'Baseline-HS-20'
x, y = 1_800, 1_000
tick, markersize = 'ok', 5
plt.plot([x], [y], tick, markersize=markersize, alpha=0.90)
plt.annotate(
    name,
    (x, y),
    xytext=(0, -15),
    textcoords='offset points',
    arrowprops={'arrowstyle': '-', 'alpha': 0.3},
    alpha=0.75,
    fontsize='small'
)

# Batched HS
name = 'Batched-HS-20'
x, y = 50_000, 1_200
tick, markersize = 'ok', 5
plt.plot([x], [y], tick, markersize=markersize, alpha=0.90)
plt.annotate(
    name,
    (x, y),
    xytext=(0, -15),
    textcoords='offset points',
    arrowprops={'arrowstyle': '-', 'alpha': 0.3},
    alpha=0.75,
    fontsize='small'
)

# Narwhal HS
name = 'Narwhal-HS-20'
x, y = 140_000, 1_800
tick, markersize = '*y', 12
plt.plot([x], [y], tick, markersize=markersize, alpha=0.90)
plt.annotate(
    name,
    (x, y),
    xytext=(-65, -15),
    textcoords='offset points',
    arrowprops={'arrowstyle': '-', 'alpha': 0.3},
    alpha=0.75,
    fontsize='small'
)

name = 'Narwhal-HS-4W10'
x, y = 240_000, 2_000
tick, markersize = '+r', 12
plt.plot([x], [y], tick, markersize=markersize, alpha=0.90)
plt.annotate(
    name,
    (x, y),
    xytext=(-80, -15),
    textcoords='offset points',
    arrowprops={'arrowstyle': '-', 'alpha': 0.3},
    alpha=0.75,
    fontsize='small'
)


# Tusk
name = 'Tusk-20'
x, y = 160_000, 3_000
tick, markersize = '*y', 12
plt.plot([x], [y], tick, markersize=markersize, alpha=0.90)
plt.annotate(
    name,
    (x, y),
    xytext=(-35, -15),
    textcoords='offset points',
    arrowprops={'arrowstyle': '-', 'alpha': 0.3},
    alpha=0.75,
    fontsize='small'
)

name = 'Tusk-4W10'
x, y = 240_000, 3_000
tick, markersize = '+r', 12
plt.plot([x], [y], tick, markersize=markersize, alpha=0.90)
plt.annotate(
    name,
    (x, y),
    xytext=(-50, -15),
    textcoords='offset points',
    arrowprops={'arrowstyle': '-', 'alpha': 0.3},
    alpha=0.75,
    fontsize='small'
)


plt.ylim(bottom=0, top=3_500)
plt.xlabel('Throughput (tx/s)')
plt.ylabel('Latency (s)')
plt.grid(True)
ax = plt.gca()
ax.xaxis.set_major_formatter(major_formatter)
ax.yaxis.set_major_formatter(major_formatter_sec)

for x in ['pdf', 'png']:
    plt.savefig(f'summary-latency.{x}', bbox_inches='tight')
