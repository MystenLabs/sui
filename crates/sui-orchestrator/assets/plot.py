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
# matplotlib as dependency: `pip install matplotlib`.


def aggregate_tps(measurement):
    tps = []
    for data in measurement['scrapers'].values():
        count = float(data[-1]['count'])
        duration = float(data[-1]['timestamp']['secs'])
        tps += [(count / duration) if duration != 0 else 0]
    return sum(tps)


def aggregate_average_latency(measurement):
    latency = []
    for data in measurement['scrapers'].values():
        last = data[-1]
        count = float(last['count'])
        total = float(last['sum']['secs'])
        latency += [total / count if count != 0 else 0]
    return sum(latency) / len(latency) if latency else 0


def aggregate_stdev_latency(measurement):
    stdev = []
    for data in measurement['scrapers'].values():
        last = data[-1]
        count = float(last['count'])
        if count == 0:
            stdev += [0]
        else:
            first_term = float(last['squared_sum']['secs']) / count
            second_term = (float(last['sum']['secs']) / count)**2
            stdev += [math.sqrt(first_term - second_term)]
    return max(stdev)


class PlotType(Enum):
    L_GRAPH = 1
    HEALTH = 2
    SLA = 3  # TODO


class PlotError(Exception):
    pass


@tick.FuncFormatter
def default_major_formatter(x, pos):
    if pos is None:
        return
    return f'{x/1000:.0f}k' if x >= 10_000 else f'{x:,.0f}'


@tick.FuncFormatter
def sec_major_formatter(x, pos):
    if pos is None:
        return
    return f'{x:,.0f}' if x >= 10 else f'{x:,.1f}'


class PlotParameters:
    def __init__(self, shared_objects_ratio, nodes, faults, specs=None, commit=None):
        self.nodes = nodes
        self.faults = faults
        self.shared_objects_ratio = shared_objects_ratio
        self.specs = specs
        self.commit = commit


class MeasurementId:
    def __init__(self, measurement):
        self.shared_objects_ratio = measurement['parameters']['shared_objects_ratio']
        self.nodes = measurement['parameters']['nodes']
        self.faults = measurement['parameters']['faults']
        self.duration = measurement['parameters']['duration']
        self.machine_specs = measurement['machine_specs']
        self.commit = measurement['commit']


class Plotter:

    def __init__(self, data_directory, parameters, y_max=None, legend_columns=2):
        self.data_directory = data_directory
        self.parameters = parameters
        self.y_max = y_max
        self.legend_columns = legend_columns

    def _make_plot_directory(self):
        plot_directory = os.path.join(self.data_directory, 'plots')
        if not os.path.exists(plot_directory):
            os.makedirs(plot_directory)

        return plot_directory

    def _legend_entry(self, plot_type, id):
        if plot_type == PlotType.L_GRAPH or plot_type == PlotType.HEALTH:
            f = id.faults
            l = f'{id.nodes} nodes' if f == 0 else f'{id.nodes} ({f} faulty)'
            return f'{l} - {id.shared_objects_ratio}% shared objects'
        else:
            assert False

    def _axes_labels(self, plot_type):
        if plot_type == PlotType.L_GRAPH:
            return ('Throughput (tx/s)', 'Latency (s)')
        elif plot_type == PlotType.HEALTH:
            return ('Input Load (tx/s)', 'Throughput (tx/s)')
        else:
            assert False

    def _plot(self, data, plot_type):
        plt.figure(figsize=(6.4, 2.4))
        markers = cycle(['o', 'v', 's', 'p', 'D', 'P'])

        for id, x_values, y_values, e_values in data:
            plt.errorbar(
                x_values, y_values, yerr=e_values,
                label=self._legend_entry(plot_type, id),
                linestyle='dotted', marker=next(markers), capsize=3
            )

        if plot_type == PlotType.L_GRAPH:
            legend_anchor = (0, 1)
            legend_location = 'upper left'
            x_label, y_label = self._axes_labels(plot_type)
            plot_name = f'latency-{self.parameters.shared_objects_ratio}'
        elif plot_type == PlotType.HEALTH:
            legend_anchor = (0, 1)
            legend_location = 'upper left'
            x_label, y_label = self._axes_labels(plot_type)
            plot_name = f'health-{self.parameters.shared_objects_ratio}'
        else:
            assert False

        plt.legend(
            loc=legend_location,
            bbox_to_anchor=legend_anchor,
            ncol=self.legend_columns
        )
        plt.xlim(xmin=0)
        plt.ylim(bottom=0, top=self.y_max)
        plt.xlabel(x_label, fontweight='bold')
        plt.ylabel(y_label, fontweight='bold')
        plt.xticks(weight='bold')
        plt.yticks(weight='bold')
        plt.grid()
        ax = plt.gca()
        ax.xaxis.set_major_formatter(default_major_formatter)
        ax.yaxis.set_major_formatter(default_major_formatter)
        if plot_type == PlotType.L_GRAPH:
            ax.yaxis.set_major_formatter(sec_major_formatter)

        for x in ['pdf', 'png']:
            filename = os.path.join(
                self._make_plot_directory(), f'{plot_name}.{x}'
            )
            plt.savefig(filename, bbox_inches='tight')

    def _load_measurement_data(self, filename):
        measurements = []
        files = glob(os.path.join(self.data_directory, filename))
        for file in files:
            with open(file, 'r') as f:
                try:
                    measurements += [json.loads(f.read())]
                except json.JSONDecodeError as e:
                    raise PlotError(f'Failed to load file {files}: {e}')

        return measurements

    def _file_format(self, shared_objects_ratio, faults, nodes, load):
        return f'measurements-{shared_objects_ratio}-{faults}-{nodes}-{load}.json'

    def plot_latency_throughput(self):
        plot_lines_data = []
        shared_objects_ratio = self.parameters.shared_objects_ratio
        for n in self.parameters.nodes:
            for f in self.parameters.faults:
                filename = self._file_format(shared_objects_ratio, f, n, '*')
                plot_lines_data += [self._load_measurement_data(filename)]

        plot_data = []
        for measurements in plot_lines_data:
            x_values, y_values, e_values = [], [], []
            measurements.sort(key=lambda x: x['parameters']['load'])
            for measurement in measurements:
                x_values += [aggregate_tps(measurement)]
                y_values += [aggregate_average_latency(measurement)]
                e_values += [aggregate_stdev_latency(measurement)]

            if x_values:
                id = MeasurementId(measurement)
                plot_data += [(id, x_values, y_values, e_values)]

        self._plot(plot_data, PlotType.L_GRAPH)

    def plot_health(self):
        plot_lines_data = []
        shared_objects_ratio = self.parameters.shared_objects_ratio
        for n in self.parameters.nodes:
            for f in self.parameters.faults:
                filename = self._file_format(shared_objects_ratio, f, n, '*')
                plot_lines_data += [self._load_measurement_data(filename)]

        plot_data = []
        for measurements in plot_lines_data:
            x_values, y_values, e_values = [], [], []
            measurements.sort(key=lambda x: x['parameters']['load'])
            for measurement in measurements:
                x_values += [measurement['parameters']['load']]
                y_values += [aggregate_tps(measurement)]
                e_values += [0]

            if x_values:
                id = MeasurementId(measurements[0])
                plot_data += [(id, x_values, y_values, e_values)]

        self._plot(plot_data, PlotType.HEALTH)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        prog='Sui Plotter',
        description='Simple script to plot measurement data'
    )
    parser.add_argument(
        '--dir', default='../../../results', help='Data directory'
    )
    parser.add_argument(
        '--shared-objects-ratio', nargs='+', type=int, required=True,
        help='The ratio of shared objects to plot (in separate graphs)'
    )
    parser.add_argument(
        '--nodes', nargs='+', type=int, required=True,
        help='The committee sizes to plot on the same graph'
    )
    parser.add_argument(
        '--faults', nargs='+', type=int, required=True,
        help='The number of faults to plot on the same graph'
    )
    parser.add_argument(
        '--y-max', type=float, default=None,
        help='The maximum value of the y-axis'
    )
    parser.add_argument(
        '--legend-columns', type=int, default=1,
        help='The number of columns of the legend'
    )
    args = parser.parse_args()

    for r in args.shared_objects_ratio:
        parameters = PlotParameters(r, args.nodes, args.faults)
        plotter = Plotter(
            args.dir, parameters, args.y_max, args.legend_columns
        )
        plotter.plot_latency_throughput()
        plotter.plot_health()
