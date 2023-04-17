import pandas as pd
import matplotlib.pyplot as plt
import numpy as np
import mpld3
import os
import glob


# use glob to get all the csv files
# in the folder
path = os.getcwd()
csv_files = glob.glob(os.path.join(path, "outputs/*.csv"))

fig, ax = plt.subplots()

ax.grid(color='white', linestyle='solid')
ax.set_title("Comparison of gas and execution time for old, new (and possibly previous) cost models", size=20)
ax.set_xlabel("Time (Nanoseconds)", size = 20)
ax.set_ylabel("Gas", size = 20)
labels = ["Old Gas Model"]

old = pd.read_csv('outputs/old_gas_model.csv')
ax.scatter(old.nanos.values,
           old.gas.values,
           c='black',
           label= "Old Gas Model",
           alpha=0.8)
tooltip = mpld3.plugins.PointHTMLTooltip(ax.collections[0], labels= ['old {0} (gas: {1}, nanos: {2})'.format(row['name'], row.gas, row.nanos) for _, row in old.iterrows()])
mpld3.plugins.connect(fig, tooltip)


# loop over the list of csv files
for idx, f in enumerate(csv_files):

    file_name = f.split("/")[-1]
    labels.append(file_name)

    # read the csv file
    df = pd.read_csv(f)
    ax.scatter(df.nanos.values,
               df.gas.values,
               label= file_name,
               alpha=0.8)

    tooltip = mpld3.plugins.PointHTMLTooltip(ax.collections[idx + 1], labels= ['{3} {0} (gas: {1}, nanos: {2})'.format(row['name'], row.gas, row.nanos, file_name) for _, row in df.iterrows()])
    mpld3.plugins.connect(fig, tooltip)

    # print the location and filename
    print('Plotting:', f.split("/")[-1])

handles, llabels = ax.get_legend_handles_labels() # return lines and labels
interactive_legend = mpld3.plugins.InteractiveLegendPlugin(
    zip(handles, ax.collections), labels, alpha_unsel=0.0, alpha_over=5,
    start_visible=True)

mpld3.plugins.connect(fig, interactive_legend)

mpld3.save_html(fig, "rough_prelim_results.html")
mpld3.show()
