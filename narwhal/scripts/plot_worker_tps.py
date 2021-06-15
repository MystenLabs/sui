# Copyright (c) Facebook, Inc. and its affiliates.
from re import findall, search, split
from glob import glob
import matplotlib.pyplot as plt
from matplotlib.ticker import MaxNLocator, StrMethodFormatter
from os.path import join
from statistics import mean
import sys
from itertools import cycle
import matplotlib.ticker as ticker


# FuncFormatter can be used as a decorator
@ticker.FuncFormatter
def major_formatter(x, pos):
    return f"{x/1000:0.0f}k"


class Ploter:
    def __init__(self, results):
        assert isinstance(results, list) and results
        assert all(isinstance(x, str) for x in results)
        results.sort(key=self._natural_keys)
        self.results = [x.replace(',', '') for x in results]

    def _natural_keys(self, text):
        def try_cast(text): return int(text) if text.isdigit() else text
        return [try_cast(c) for c in split('(\d+)', text)]

    def _tps(self, data):
        values = findall(r' TPS: (\d+) \+/- (\d+)', data)
        values = [(int(x), int(y)) for x, y in values]
        return list(zip(*values))

    def _latency(self, data):
        values = findall(r' Latency: (\d+) \+/- (\d+)', data)
        values = [(float(x)/1000, float(y)/1000) for x, y in values]
        return list(zip(*values))

    def _variable(self, data):
        val = findall(r'Variable value: X=[0-9]*', data)
        val = findall(r'\d+', ''.join(val))
        return [int(x) for x in val]

    def _tx_size(self, data=None):
        data = self.results[0] if data is None else data
        return int(search(r'Transaction size: (\d+)', data).group(1))

    def _tps2bps(self, x):
        return x * self._tx_size() / 10**6

    def _bps2tps(self, x):
        return x * 10**6 / self._tx_size()

    def workers(self, data):
        x = search(r'Number of workers: (\d+)', data).group(1)
        return f'{x} workers'

    def nodes(self, data):
        x = search(r'Committee size: (\d+)', data).group(1)
        f = search(r'Faults: (\d+)', data).group(1)
        faults = f'({f} faulty)' if f != '0' else ''
        return f'{x} nodes {faults}'

    def tx_size(self, data):
        return f'Transaction size: {self._tx_size(data=data):,} B'

    def batch_size(self, data):
        x = search(r'Batch size: (\d+)', data).group(1)
        return f'Batch size: {int(x):,} txs'

    def bench_type(self, data):
        label = search(r'.* Benchmark', data).group(0).split(' ')[1]
        return label.strip().capitalize()

    def max_latency(self, data):
        x = search(r'Max latency: (\d+)', data).group(1)
        f = search(r'Faults: (\d+)', data).group(1)
        faults = f'({f} faulty)' if f != '0' else ''
        return f'Max latency: {float(x) / 1000:,.1f} s {faults}'

    def _plot(self, xlabel, ylabel, y_axis, z_axis, filename):
        markers = cycle(['o', 'v', 's'])
        # plt.figure()
        for result in self.results:
            y_values, y_err = y_axis(result)
            x_values = self._variable(result)
            assert len(y_values) == len(y_err) and len(y_err) == len(x_values)
            plt.errorbar(
                x_values, y_values, yerr=y_err,  # uplims=True, lolims=True,
                marker=next(markers), label=z_axis(result), linestyle='dotted'
            )
            # plt.yscale('log')

        plt.legend(loc='lower center', bbox_to_anchor=(0.5, 1), ncol=3)
        # plt.xlim(xmin=0)
        # plt.ylim(bottom=0, top=8)
        #plt.ylim = [0, 10]
        plt.grid(True, which='both')
        plt.xlabel(xlabel)
        plt.ylabel(ylabel[0])
        ax = plt.gca()
        #ax.ticklabel_format(useOffset=False, style='plain')
        # ax.xaxis.set_major_locator(MaxNLocator(integer=True))
        ax.xaxis.set_major_formatter(StrMethodFormatter('{x:,.0f}'))
        ax.yaxis.set_major_formatter(major_formatter)
        if len(ylabel) > 1:
            secaxy = ax.secondary_yaxis(
                'right', functions=(self._tps2bps, self._bps2tps)
            )
            secaxy.set_ylabel(ylabel[1])
            secaxy.yaxis.set_major_formatter(StrMethodFormatter('{x:,.0f}'))

        plt.savefig(f'plot/{filename}.pdf', bbox_inches='tight')
        plt.savefig(f'plot/{filename}.png', bbox_inches='tight')

    def plot_tps(self, xlabel, z_axis):
        assert isinstance(xlabel, str)
        assert hasattr(z_axis, '__call__')
        ylabel = ['Throughput (tx/s)', 'Throughput (MB/s)']
        self._plot(xlabel, ylabel, self._tps, z_axis, 'tps')

    def plot_client_latency(self, z_axis):
        assert hasattr(z_axis, '__call__')
        xlabel = 'Throughput (tx/s)'
        ylabel = ['Client latency (s)']
        self._plot(
            xlabel, ylabel, self._latency, z_axis, 'latency'
        )

    def plot_robustness(self, z_axis):
        assert hasattr(z_axis, '__call__')
        x_label = 'Input rate (tx/s)'
        y_label = ['Throughput (tx/s)', 'Throughput (MB/s)']
        self._plot(x_label, y_label, self._tps,
                   z_axis, 'robustness')


if __name__ == '__main__':
    plt.figure(figsize=[5.2, 2.4])
    results = []
    name = 'agg-4-x-0-512-1000-any-*.txt'
    # name = 'agg-4-*-0-512-1000-any-any.txt'
    for filename in glob(join(sys.argv[1], name)):
        with open(filename, 'r') as f:
            results += [f.read()]

    #plt.ylim(bottom=0, top=8)
    ploter = Ploter(results)
    ploter.plot_tps('Workers per validator', ploter.max_latency)
    # ploter.plot_tps('Committee size', ploter.tx_size)
    #ploter.plot_client_latency(ploter.workers)
