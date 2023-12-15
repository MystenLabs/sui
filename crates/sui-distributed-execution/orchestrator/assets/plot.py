# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

import argparse
from enum import Enum
import glob
import json
import math
import os
import matplotlib.pyplot as plt
import matplotlib.ticker as tick
from glob import glob
from itertools import cycle

# A simple python script to plot measurements results. This script requires
# the following dependencies: `pip install matplotlib`.


def ramp_up(scraper, ramp_up_threshold=5):
    # ramp_up_duration, ramp_up_count = 0, 0
    # ramp_up_sum, ramp_up_square_sum = 0, 0
    # for data in scraper:
    #     duration = float(data["timestamp"]["secs"])
    #     if duration > ramp_up_threshold:
    #         ramp_up_duration = duration
    #         ramp_up_count = float(data["count"])
    #         ramp_up_sum = float(data["sum"]["secs"])
    #         ramp_up_square_sum = float(data["squared_sum"]["secs"])
    #         break
    # return ramp_up_duration, ramp_up_count, ramp_up_sum, ramp_up_square_sum
    return 0, 0, 0, 0


def aggregate_tps(measurement, workload):
    if workload not in measurement["data"]:
        return 0

    max_duration = 0
    for data in measurement["data"][workload].values():
        ramp_up_duration, _, _, _ = ramp_up(data)
        duration = float(data[-1]["timestamp"]["secs"]) - ramp_up_duration
        max_duration = max(duration, max_duration)

    tps = []
    for data in measurement["data"][workload].values():
        _, ramp_up_count, _, _ = ramp_up(data)
        count = float(data[-1]["count"]) - ramp_up_count
        tps += [(count / max_duration) if max_duration != 0 else 0]
    return sum(tps)


def aggregate_average_latency(measurement, workload):
    if workload not in measurement["data"]:
        return 0

    latency = []
    for data in measurement["data"][workload].values():
        _, ramp_up_count, ramp_up_sum, _ = ramp_up(data)
        last = data[-1]
        count = float(last["count"]) - ramp_up_count
        total = float(last["sum"]["secs"]) - ramp_up_sum
        latency += [total / count if count != 0 else 0]
    return sum(latency) / len(latency) if latency else 0


def aggregate_stdev_latency(measurement, workload):
    if workload not in measurement["data"]:
        return 0

    stdev = []
    for data in measurement["data"][workload].values():
        _, ramp_up_count, ramp_up_sum, ramp_up_square_sum = ramp_up(data)
        last = data[-1]
        count = float(last["count"]) - ramp_up_count
        if count == 0:
            stdev += [0]
        else:
            latency_sum = float(last["sum"]["secs"]) - ramp_up_sum
            latency_square_sum = float(last["squared_sum"]["secs"]) - ramp_up_square_sum

            first_term = latency_square_sum / count
            second_term = (latency_sum / count) ** 2
            if round(first_term - second_term) > 0:
                stdev += [math.sqrt(first_term - second_term)]
            else:
                stdev += [0]
    return max(stdev)


def aggregate_p_latency(measurement, workload, p=50, i=-1):
    if workload not in measurement["data"]:
        return 0

    latency = []
    for data in measurement["data"][workload].values():
        last = data[i]
        count = float(last["count"])
        buckets = [(float(l), c) for l, c in last["buckets"].items()]
        buckets.sort(key=lambda x: x[0])

        for l, c in buckets:
            if c >= count * p / 100:
                latency += [l]
                break
    return sum(latency) / len(latency) if latency else 0


class PlotType(Enum):
    L_GRAPH = 1
    HEALTH = 2
    SCALABILITY = 3
    INSPECT_TPS = 4
    INSPECT_LATENCY = 5
    DURATION_TPS = 6
    DURATION_LATENCY = 7


class PlotError(Exception):
    pass


@tick.FuncFormatter
def default_major_formatter(x, pos):
    if pos is None:
        return
    return f"{x/1000:.0f}k" if x >= 10_000 else f"{x:,.0f}"


@tick.FuncFormatter
def ms_major_formatter(x, pos):
    if pos is None:
        return
    # return f"{x:,.0f}" if x >= 10 else f"{x:,.1f}"
    return int(x * 1000)


class PlotParameters:
    def __init__(self, seq_workers, exec_workers, faults, specs=None, commit=None):
        self.seq_workers = seq_workers
        self.exec_workers = exec_workers
        self.faults = faults
        self.specs = specs
        self.commit = commit


class MeasurementId:
    def __init__(self, measurement, workload, max_latency=None):
        self.seq_workers = measurement["parameters"]["benchmark_type"][
            "sequence_workers"
        ]
        self.exec_workers = measurement["parameters"]["benchmark_type"][
            "execution_workers"
        ]
        if "Permanent" in measurement["parameters"]["faults"]:
            self.faults = measurement["parameters"]["faults"]["Permanent"]["faults"]
        else:
            self.faults = 0
        self.duration = measurement["parameters"]["duration"]
        self.machine_specs = measurement["machine_specs"]
        self.commit = measurement["commit"]

        self.workload = workload
        self.max_latency = max_latency


class Plotter:
    def __init__(
        self, data_directory, parameters, y_max=None, legend_columns=2, median=True
    ):
        self.data_directory = data_directory
        self.parameters = parameters
        self.y_max = y_max
        self.legend_columns = legend_columns
        self.median = median

    def _make_plot_directory(self):
        plot_directory = os.path.join(self.data_directory, "plots")
        if not os.path.exists(plot_directory):
            os.makedirs(plot_directory)

        return plot_directory

    def _legend_entry(self, plot_type, id):
        if plot_type == PlotType.L_GRAPH:
            f = "" if id.faults == 0 else f" ({id.faults} faulty)"
            l = f"{id.exec_workers} EW{f}"
            return f"{l} - {id.workload} tx)"
        elif plot_type == PlotType.SCALABILITY:
            f = "" if id.faults == 0 else f" ({id.faults} faulty)"
            l = f"{id.max_latency}s latency cap{f}"
            return f"{l} - {id.workload} tx)"
        else:
            return None

    def _axes_labels(self, plot_type):
        if plot_type == PlotType.L_GRAPH:
            return ("Throughput (tx/s)", "Latency (ms)")
        elif plot_type == PlotType.SCALABILITY:
            return ("EW", "Throughput (tx/s)")
        else:
            assert False

    def _plot(self, data, plot_type):
        plt.figure(figsize=(6.4, 2.4))
        markers = cycle(["o", "v", "s", "p", "D", "P"])

        for id, x_values, y_values, e_values in data:
            plt.errorbar(
                x_values,
                y_values,
                yerr=e_values,
                label=self._legend_entry(plot_type, id),
                linestyle="dotted",
                marker=next(markers),
                capsize=3,
            )

        if plot_type == PlotType.L_GRAPH:
            legend_anchor, legend_location = (0.5, 1), "lower center"
            plot_name = f"latency"
        elif plot_type == PlotType.SCALABILITY:
            legend_anchor, legend_location = (0, 0), "lower left"
            plot_name = f"scalability"
        else:
            assert False

        x_label, y_label = self._axes_labels(plot_type)

        if data:
            plt.legend(
                loc=legend_location,
                bbox_to_anchor=legend_anchor,
                ncol=self.legend_columns,
            )
        plt.xlim(xmin=0)
        plt.ylim(bottom=0)
        plt.ylim(top=0.03)
        if plot_type == PlotType.L_GRAPH:
            plt.ylim(top=self.y_max)
        plt.xlabel(x_label, fontweight="bold")
        plt.ylabel(y_label, fontweight="bold")
        plt.xticks(weight="bold")
        plt.yticks(weight="bold")
        plt.grid()
        ax = plt.gca()
        ax.xaxis.set_major_formatter(default_major_formatter)
        ax.yaxis.set_major_formatter(default_major_formatter)
        if plot_type in [PlotType.L_GRAPH, PlotType.INSPECT_LATENCY]:
            ax.yaxis.set_major_formatter(ms_major_formatter)

        for x in ["pdf", "png"]:
            filename = os.path.join(self._make_plot_directory(), f"{plot_name}.{x}")
            plt.savefig(filename, bbox_inches="tight")

    def _load_measurement_data(self, filename):
        measurements = []
        files = glob(os.path.join(self.data_directory, filename))
        for file in files:
            with open(file, "r") as f:
                try:
                    measurements += [json.loads(f.read())]
                except json.JSONDecodeError as e:
                    raise PlotError(f"Failed to load file {file}: {e}")

        return measurements

    def _file_format(self, seq_workers, exec_workers, faults, load):
        return f"measurements-{seq_workers}-{exec_workers}-{faults}-{exec_workers}-{load}.json"

    def plot_latency_throughput(self, workload):
        plot_lines_data = []
        seq_workers = self.parameters.seq_workers
        for n in self.parameters.exec_workers:
            for f in self.parameters.faults:
                filename = self._file_format(seq_workers, n, f, "*")
                plot_lines_data += [self._load_measurement_data(filename)]

        plot_data = []
        for measurements in plot_lines_data:
            for w in workload:
                x_values, y_values, e_values = [], [], []
                measurements.sort(key=lambda x: x["parameters"]["load"])
                for measurement in measurements:
                    x_values += [aggregate_tps(measurement, w)]
                    if self.median:
                        y_values += [aggregate_p_latency(measurement, w, p=50)]
                        e_values += [aggregate_p_latency(measurement, w, p=75)]
                    else:
                        y_values += [aggregate_average_latency(measurement, w)]
                        e_values += [aggregate_stdev_latency(measurement, w)]

                if x_values:
                    id = MeasurementId(measurements[0], w)
                    plot_data += [(id, x_values, y_values, e_values)]

        self._plot(plot_data, PlotType.L_GRAPH)

    # def plot_scalability(self, max_latencies, workload):
    #     plot_data = []

    #     for w in workload:
    #         plot_lines_data = []
    #         transaction_size = self.parameters.transaction_size
    #         for f in self.parameters.faults:
    #             for l in max_latencies:
    #                 filenames = []
    #                 for n in self.parameters.nodes:
    #                     filename = self._file_format(transaction_size, f, n, "*")
    #                     measurements = self._load_measurement_data(filename)
    #                     measurements = [
    #                         x
    #                         for x in measurements
    #                         if aggregate_average_latency(x, w) <= l
    #                     ]
    #                     if measurements:
    #                         filenames += [
    #                             max(measurements, key=lambda x: aggregate_tps(x, w))
    #                         ]
    #                 plot_lines_data += [(filenames, l)]

    #         for measurements, max_latency in plot_lines_data:
    #             x_values, y_values, e_values = [], [], []
    #             for measurement in measurements:
    #                 x_values += [measurement["parameters"]["nodes"]]
    #                 y_values += [aggregate_tps(measurement, w)]
    #                 e_values += [0]

    #             if x_values:
    #                 id = MeasurementId(measurements[0], w, max_latency)
    #                 plot_data += [(id, x_values, y_values, e_values)]

    #     self._plot(plot_data, PlotType.SCALABILITY)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        prog="Sui Plotter", description="Simple script to plot measurement data"
    )
    parser.add_argument("--dir", default="./", help="Data directory")
    parser.add_argument(
        "--seq-workers",
        nargs="+",
        type=int,
        default=[1],
        help="The size of each transaction in the benchmark",
    )
    parser.add_argument(
        "--workload",
        nargs="+",
        type=str,
        default=["default"],
        help="The type of object transaction (owned or shared)",
    )
    parser.add_argument(
        "--exec-workers",
        nargs="+",
        type=int,
        default=[4],
        help="The exec workers to plot on the same graph",
    )
    parser.add_argument(
        "--faults",
        nargs="+",
        type=int,
        default=[0],
        help="The number of faults to plot on the same graph",
    )
    parser.add_argument(
        "--max-latencies",
        nargs="+",
        type=float,
        default=[1, 2],
        help="The latency cap (in seconds) for scalability graphs",
    )
    parser.add_argument(
        "--y-max",
        type=float,
        default=None,
        help="The maximum value of the y-axis for L-graphs",
    )
    parser.add_argument(
        "--legend-columns",
        type=int,
        default=1,
        help="The number of columns of the legend",
    )
    args = parser.parse_args()

    for r in args.seq_workers:
        parameters = PlotParameters(r, args.exec_workers, args.faults)
        plotter = Plotter(
            args.dir, parameters, args.y_max, args.legend_columns, median=True
        )
        plotter.plot_latency_throughput(args.workload)
        # plotter.plot_scalability(args.max_latencies, args.workload)
