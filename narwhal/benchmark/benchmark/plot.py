# Copyright(C) Facebook, Inc. and its affiliates.
from collections import defaultdict
from re import findall, search, split
import matplotlib.pyplot as plt
import matplotlib.ticker as tick
from glob import glob
from itertools import cycle

from benchmark.utils import PathMaker
from benchmark.config import PlotParameters
from benchmark.aggregate import LogAggregator


@tick.FuncFormatter
def default_major_formatter(x, pos):
    if pos is None:
        return
    if x >= 1_000:
        return f'{x/1000:.0f}k'
    else:
        return f'{x:.0f}'


@tick.FuncFormatter
def sec_major_formatter(x, pos):
    if pos is None:
        return
    return f'{float(x)/1000:.1f}'


@tick.FuncFormatter
def mb_major_formatter(x, pos):
    if pos is None:
        return
    return f'{x:,.0f}'


class PlotError(Exception):
    pass


class Ploter:
    def __init__(self, filenames):
        if not filenames:
            raise PlotError('No data to plot')

        self.results = []
        try:
            for filename in filenames:
                with open(filename, 'r') as f:
                    self.results += [f.read().replace(',', '')]
        except OSError as e:
            raise PlotError(f'Failed to load log files: {e}')

    def _natural_keys(self, text):
        def try_cast(text): return int(text) if text.isdigit() else text
        return [try_cast(c) for c in split('(\d+)', text)]

    def _tps(self, data):
        values = findall(r' TPS: (\d+) \+/- (\d+)', data)
        values = [(int(x), int(y)) for x, y in values]
        return list(zip(*values))

    def _latency(self, data, scale=1):
        values = findall(r' Latency: (\d+) \+/- (\d+)', data)
        values = [(float(x)/scale, float(y)/scale) for x, y in values]
        return list(zip(*values))

    def _variable(self, data):
        return [int(x) for x in findall(r'Variable value: X=(\d+)', data)]

    def _tps2bps(self, x):
        data = self.results[0]
        size = int(search(r'Transaction size: (\d+)', data).group(1))
        return x * size / 10**6

    def _bps2tps(self, x):
        data = self.results[0]
        size = int(search(r'Transaction size: (\d+)', data).group(1))
        return x * 10**6 / size

    def _plot(self, x_label, y_label, y_axis, z_axis, type):
        plt.figure()
        markers = cycle(['o', 'v', 's', 'p', 'D', 'P'])
        self.results.sort(key=self._natural_keys, reverse=(type == 'tps'))
        for result in self.results:
            y_values, y_err = y_axis(result)
            x_values = self._variable(result)
            if len(y_values) != len(y_err) or len(y_err) != len(x_values):
                raise PlotError('Unequal number of x, y, and y_err values')

            plt.errorbar(
                x_values, y_values, yerr=y_err, label=z_axis(result),
                linestyle='dotted', marker=next(markers), capsize=3
            )

        plt.legend(loc='lower center', bbox_to_anchor=(0.5, 1), ncol=3)
        plt.xlim(xmin=0)
        plt.ylim(bottom=0)
        plt.xlabel(x_label, fontweight='bold')
        plt.ylabel(y_label[0], fontweight='bold')
        plt.xticks(weight='bold')
        plt.yticks(weight='bold')
        plt.grid()
        ax = plt.gca()
        ax.xaxis.set_major_formatter(default_major_formatter)
        ax.yaxis.set_major_formatter(default_major_formatter)
        if 'latency' in type:
            ax.yaxis.set_major_formatter(sec_major_formatter)
        if len(y_label) > 1:
            secaxy = ax.secondary_yaxis(
                'right', functions=(self._tps2bps, self._bps2tps)
            )
            secaxy.set_ylabel(y_label[1])
            secaxy.yaxis.set_major_formatter(mb_major_formatter)

        for x in ['pdf', 'png']:
            plt.savefig(PathMaker.plot_file(type, x), bbox_inches='tight')

    @staticmethod
    def nodes(data):
        x = search(r'Committee size: (\d+)', data).group(1)
        f = search(r'Faults: (\d+)', data).group(1)
        faults = f'({f} faulty)' if f != '0' else ''
        return f'{x} nodes {faults}'

    @staticmethod
    def workers(data):
        x = search(r'Workers per node: (\d+)', data).group(1)
        f = search(r'Faults: (\d+)', data).group(1)
        faults = f'({f} faulty)' if f != '0' else ''
        return f'{x} workers {faults}'

    @staticmethod
    def max_latency(data):
        x = search(r'Max latency: (\d+)', data).group(1)
        f = search(r'Faults: (\d+)', data).group(1)
        faults = f'({f} faulty)' if f != '0' else ''
        return f'Max latency: {float(x) / 1000:,.1f} s {faults}'

    @classmethod
    def plot_latency(cls, files, scalability):
        assert isinstance(files, list)
        assert all(isinstance(x, str) for x in files)
        z_axis = cls.workers if scalability else cls.nodes
        x_label = 'Throughput (tx/s)'
        y_label = ['Latency (s)']
        ploter = cls(files)
        ploter._plot(x_label, y_label, ploter._latency, z_axis, 'latency')

    @classmethod
    def plot_tps(cls, files, scalability):
        assert isinstance(files, list)
        assert all(isinstance(x, str) for x in files)
        z_axis = cls.max_latency
        x_label = 'Workers per node' if scalability else 'Committee size'
        y_label = ['Throughput (tx/s)', 'Throughput (MB/s)']
        ploter = cls(files)
        ploter._plot(x_label, y_label, ploter._tps, z_axis, 'tps')

    @classmethod
    def plot(cls, params_dict):
        try:
            params = PlotParameters(params_dict)
        except PlotError as e:
            raise PlotError('Invalid nodes or bench parameters', e)

        # Aggregate the logs.
        LogAggregator(params.max_latency).print()

        # Make the latency, tps, and robustness graphs.
        iterator = params.workers if params.scalability() else params.nodes
        latency_files, tps_files = [], []
        for f in params.faults:
            for x in iterator:
                latency_files += glob(
                    PathMaker.agg_file(
                        'latency',
                        f,
                        x if not params.scalability() else params.nodes[0],
                        x if params.scalability() else params.workers[0],
                        params.collocate,
                        'any',
                        params.tx_size,
                    )
                )

            for l in params.max_latency:
                tps_files += glob(
                    PathMaker.agg_file(
                        'tps',
                        f,
                        'x' if not params.scalability() else params.nodes[0],
                        'x' if params.scalability() else params.workers[0],
                        params.collocate,
                        'any',
                        params.tx_size,
                        max_latency=l
                    )
                )

        cls.plot_latency(latency_files, params.scalability())
        cls.plot_tps(tps_files, params.scalability())
