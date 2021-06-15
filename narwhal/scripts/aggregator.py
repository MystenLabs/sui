# Copyright (c) Facebook, Inc. and its affiliates.
from re import search
from collections import defaultdict
from statistics import mean, stdev
from copy import deepcopy


class Setup:
    def __init__(self, nodes, workers, faults, tx_size, batch_size, rate):
        self.nodes = nodes
        self.workers = workers
        self.faults = faults
        self.tx_size = tx_size
        self.batch_size = batch_size
        self.rate = rate
        self.max_latency = 'any'

    def __str__(self):
        return (
            f' Committee size: {self.nodes} nodes\n'
            f' Number of workers: {self.workers} worker(s) per node\n'
            f' Faults: {self.faults} nodes\n'
            f' Transaction size: {self.tx_size} B\n'
            f' Batch size: {self.batch_size} txs\n'
            f' Transaction rate: {self.rate} txs\n'
            f' Max latency: {self.max_latency} ms\n'
        )

    def __eq__(self, other):
        return isinstance(other, Setup) and str(self) == str(other)

    def __hash__(self):
        return hash(str(self))

    @classmethod
    def from_str(cls, raw):
        nodes = int(search(r'.* Committee size: (\d+)', raw).group(1))
        workers = int(search(r'.* Number of workers: (\d+)', raw).group(1))
        faults = int(search(r'.* Faults: (\d+)', raw).group(1))
        tx_size = int(search(r'.* Transaction size: (\d+)', raw).group(1))
        batch_size = int(search(r'.* Max batch size: (\d+)', raw).group(1))
        rate = int(search(r'.* Transaction rate: (\d+)', raw).group(1))
        return cls(nodes, workers, faults, tx_size, batch_size, rate)


class Result:
    def __init__(self, mean_tps, mean_latency, std_tps=0, std_latency=0):
        self.mean_tps = mean_tps
        self.mean_latency = mean_latency
        self.std_tps = std_tps
        self.std_latency = std_latency

    def __str__(self):
        return(
            f' TPS: {self.mean_tps} +/- {self.std_tps} tx/s\n'
            f' Latency: {self.mean_latency} +/- {self.std_latency} ms\n'
        )

    @classmethod
    def from_str(cls, raw):
        tps = int(search(r'.* Estimated TPS: (\d+)', raw).group(1))
        latency = int(search(r'.* Client Latency: (\d+)', raw).group(1))
        return cls(tps, latency)

    @classmethod
    def aggregate(cls, results):
        if len(results) == 1:
            return results[0]

        mean_tps = round(mean([x.mean_tps for x in results]))
        mean_latency = round(mean([x.mean_latency for x in results]))
        std_tps = round(stdev([x.mean_tps for x in results]))
        std_latency = round(stdev([x.mean_latency for x in results]))
        return cls(mean_tps, mean_latency, std_tps, std_latency)


class LogAggregator:
    def __init__(self, filenames):
        data = ''
        for filename in filenames:
            with open(filename, 'r') as f:
                data += f.read()

        records = defaultdict(list)
        for chunk in data.replace(',', '').split('\n\n'):
            if chunk:
                records[Setup.from_str(chunk)] += [Result.from_str(chunk)]

        self.records = {k: Result.aggregate(v) for k, v in records.items()}

    def print(self):
        results = [
            self._print_latency(), self._print_tps(), self._print_tps_workers(), self._print_robustness()
        ]
        for records in results:
            for setup, values in records.items():
                data = '\n'.join(
                    f' Variable value: X={x}\n{y}' for x, y in values
                )
                string = (
                    '\n'
                    '-----------------------------------------\n'
                    ' RESULTS:\n'
                    '-----------------------------------------\n'
                    f'{setup}'
                    '\n'
                    f'{data}'
                    '-----------------------------------------\n'
                )
                filename = f'agg-{setup.nodes}-{setup.workers}-{setup.faults}-{setup.tx_size}-{setup.batch_size}-{setup.rate}-{setup.max_latency}.txt'
                with open(filename, 'w') as f:
                    f.write(string)

    def _print_latency(self):
        records = deepcopy(self.records)
        organized = defaultdict(list)
        for setup, result in records.items():
            rate = setup.rate
            setup.rate = 'any'
            organized[setup] += [(result.mean_tps, result, rate)]

        for setup, results in list(organized.items()):
            results.sort(key=lambda x: x[2])
            organized[setup] = [(x, y) for x, y, _ in results]

        return organized

    def _print_tps(self, max_latencies=[4_000, 6_000]):
        records = deepcopy(self.records)
        organized = defaultdict(list)
        for max_latency in max_latencies:
            for setup, result in records.items():
                setup = deepcopy(setup)
                if result.mean_latency <= max_latency:
                    nodes = setup.nodes
                    setup.nodes = 'x'
                    setup.rate = 'any'
                    setup.max_latency = max_latency

                    new_point = all(nodes != x[0] for x in organized[setup])
                    highest_tps = False
                    for w, r in organized[setup]:
                        if result.mean_tps > r.mean_tps and nodes == w:
                            organized[setup].remove((w, r))
                            highest_tps = True
                    if new_point or highest_tps:
                        organized[setup] += [(nodes, result)]

        [v.sort(key=lambda x: x[0]) for v in organized.values()]
        return organized

    def _print_tps_workers(self, max_latencies=[4_000, 10_000]):
        records = deepcopy(self.records)
        organized = defaultdict(list)
        for max_latency in max_latencies:
            for setup, result in records.items():
                setup = deepcopy(setup)
                if result.mean_latency <= max_latency:
                    workers = setup.workers
                    setup.workers = 'x'
                    setup.rate = 'any'
                    setup.max_latency = max_latency

                    new_point = all(workers != x[0] for x in organized[setup])
                    highest_tps = False
                    for w, r in organized[setup]:
                        if result.mean_tps > r.mean_tps and workers == w:
                            organized[setup].remove((w, r))
                            highest_tps = True
                    if new_point or highest_tps:
                        organized[setup] += [(workers, result)]

        [v.sort(key=lambda x: x[0]) for v in organized.values()]
        return organized

    def _print_robustness(self):
        records = deepcopy(self.records)
        organized = defaultdict(list)
        for setup, result in records.items():
            rate = setup.rate
            setup.rate = 'x'
            organized[setup] += [(rate, result)]

        [v.sort(key=lambda x: x[0]) for v in organized.values()]
        return organized

