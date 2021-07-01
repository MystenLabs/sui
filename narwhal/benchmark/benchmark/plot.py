# Copyright(C) Facebook, Inc. and its affiliates.
from re import findall, search, split
import matplotlib.pyplot as plt
from matplotlib.ticker import StrMethodFormatter
from glob import glob
from itertools import cycle

from benchmark.utils import PathMaker
from benchmark.config import PlotParameters
from benchmark.aggregate import LogAggregator


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

        plt.legend(loc='lower center', bbox_to_anchor=(0.5, 1), ncol=2)
        plt.xlim(xmin=0)
        plt.ylim(bottom=0)
        plt.xlabel(x_label)
        plt.ylabel(y_label[0])
        plt.grid()
        ax = plt.gca()
        ax.xaxis.set_major_formatter(StrMethodFormatter('{x:,.0f}'))
        ax.yaxis.set_major_formatter(StrMethodFormatter('{x:,.0f}'))
        if len(y_label) > 1:
            secaxy = ax.secondary_yaxis(
                'right', functions=(self._tps2bps, self._bps2tps)
            )
            secaxy.set_ylabel(y_label[1])
            secaxy.yaxis.set_major_formatter(StrMethodFormatter('{x:,.0f}'))

        for x in ['pdf', 'png']:
            plt.savefig(PathMaker.plot_file(type, x), bbox_inches='tight')

    @staticmethod
    def nodes(data):
        x = search(r'Committee size: (\d+)', data).group(1)
        f = search(r'Faults: (\d+)', data).group(1)
        faults = f'({f} faulty)' if f != '0' else ''
        return f'{x} nodes {faults}'

    @staticmethod
    def max_latency(data):
        x = search(r'Max latency: (\d+)', data).group(1)
        f = search(r'Faults: (\d+)', data).group(1)
        faults = f'({f} faulty)' if f != '0' else ''
        return f'Max latency: {float(x) / 1000:,.1f} s {faults}'

    @classmethod
    def plot_robustness(cls, files):
        assert isinstance(files, list)
        assert all(isinstance(x, str) for x in files)
        z_axis = cls.nodes
        x_label = 'Input rate (tx/s)'
        y_label = ['Throughput (tx/s)', 'Throughput (MB/s)']
        ploter = cls(files)
        ploter._plot(x_label, y_label, ploter._tps, z_axis, 'robustness')

    @classmethod
    def plot_latency(cls, files):
        assert isinstance(files, list)
        assert all(isinstance(x, str) for x in files)
        z_axis = cls.nodes
        x_label = 'Throughput (tx/s)'
        y_label = ['Latency (ms)']
        ploter = cls(files)
        ploter._plot(x_label, y_label, ploter._latency, z_axis, 'latency')

    @classmethod
    def plot_tps(cls, files):
        assert isinstance(files, list)
        assert all(isinstance(x, str) for x in files)
        z_axis = cls.max_latency
        x_label = 'Committee size'
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

        # Load the aggregated log files.
        robustness_files, latency_files, tps_files = [], [], []        
        for f in params.faults:
            for n in params.nodes:
                robustness_files += glob(
                    PathMaker.agg_file(
                        'robustness', 
                        f, 
                        n,
                        params.workers,
                        params.collocate,
                        'x', 
                        params.tx_size,
                    )
                )
                latency_files += glob(
                    PathMaker.agg_file(
                        'latency', 
                        f,
                        n, 
                        params.workers,
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
                        'x', 
                        params.workers,
                        params.collocate,
                        'any', 
                        params.tx_size, 
                        max_latency=l
                    )
                )

        # Make the plots.
        cls.plot_robustness(robustness_files)
        cls.plot_latency(latency_files)
        cls.plot_tps(tps_files)
