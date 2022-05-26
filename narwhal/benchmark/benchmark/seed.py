# Copyright (c) 2022, Mysten Labs, Inc.
import subprocess
import re
from math import ceil
from os.path import basename, splitext
from time import sleep
from pathlib import Path
import json

from benchmark.commands import CommandMaker
from benchmark.config import Key, LocalCommittee, NodeParameters, BenchParameters, ConfigError
from benchmark.logs import LogParser, ParseError
from benchmark.utils import Print, BenchError, PathMaker


class SeedData:
    def __init__(self, bench_parameters_dict):
        try:
            self.bench_parameters = BenchParameters(bench_parameters_dict)
        except ConfigError as e:
            raise BenchError('Invalid bench parameters', e)

    def __getattr__(self, attr):
        return getattr(self.bench_parameters, attr)

    def _background_run(self, command, log_file):
        name = splitext(basename(log_file))[0]
        cmd = f'{command} 2> {log_file}'
        subprocess.run(['tmux', 'new', '-d', '-s', name, cmd], check=True)

    def _kill_nodes(self):
        try:
            cmd = CommandMaker.kill().split()
            subprocess.run(cmd, stderr=subprocess.DEVNULL)
        except subprocess.SubprocessError as e:
            raise BenchError('Failed to kill testbed', e)

    def run(self, committee_path):
        assert isinstance(committee_path, Path)
        Print.heading('Starting seeding data')

        committee_data = json.loads(open(committee_path, "r").read())
        workers_addresses = []
        transactions_address_port = 7001
        for authority in committee_data['authorities']:
            transactions_address = f'http://127.0.0.1:{transactions_address_port}/'
            transactions_address_port += 1
            print(transactions_address)
            workers_addresses.append(
                [(0, transactions_address)])

        try:
            nodes, rate = self.nodes[0], self.rate[0]

            # Run the clients (they will wait for the nodes to be ready).
            rate_share = ceil(rate / len(workers_addresses))
            for i, addresses in enumerate(workers_addresses):
                for (id, address) in addresses:
                    cmd = CommandMaker.run_client(
                        address,
                        self.tx_size,
                        rate_share,
                        [x for y in workers_addresses for _, x in y]
                    )
                    log_file = PathMaker.client_log_file(i, id)
                    self._background_run(cmd, log_file)

            # Wait for all transactions to be processed.
            Print.info(f'Seeding data ({self.duration} sec)...')
            sleep(self.duration)

        except (subprocess.SubprocessError, ParseError) as e:
            self._kill_nodes()
            raise BenchError('Failed to seed data', e)
