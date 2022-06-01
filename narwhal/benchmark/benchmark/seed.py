# Copyright (c) 2022, Mysten Labs, Inc.
import subprocess
from math import ceil
from os.path import basename, splitext
from time import sleep

from benchmark.commands import CommandMaker
from benchmark.config import BenchParameters, ConfigError
from benchmark.logs import ParseError
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

    def run(self, starting_data_port):
        assert isinstance(starting_data_port, int)
        Print.heading('Start seeding data')
        nodes, rate, workers = self.nodes[0], self.rate[0], self.workers

        workers_addresses = []
        transactions_address_port = starting_data_port
        for _ in range(nodes):
            for worker_id in range(workers):
                transactions_address = f'http://127.0.0.1:{transactions_address_port}/'
                transactions_address_port += 1
                Print.info(transactions_address)

                workers_addresses.append(
                    [(worker_id, transactions_address)])

        try:
            # Cleanup all files.
            cmd = f'{CommandMaker.clean_logs()} ; {CommandMaker.cleanup()}'
            subprocess.run([cmd], shell=True, stderr=subprocess.DEVNULL)
            sleep(0.5)  # Removing the store may take time.

            # Recompile the latest code.
            cmd = CommandMaker.compile().split()
            subprocess.run(cmd, check=True, cwd=PathMaker.node_crate_path())

            # Create alias for the client and nodes binary.
            cmd = CommandMaker.alias_binaries(PathMaker.binary_path())
            subprocess.run([cmd], shell=True)

            # Run the clients (they will wait for the nodes to be ready).
            rate_share = ceil(rate / (nodes * workers))
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
