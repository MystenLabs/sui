# Copyright(C) Facebook, Inc. and its affiliates.
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
from json import dump, load
from collections import OrderedDict
from benchmark.utils import multiaddr_to_url_data


class ConfigError(Exception):
    pass

class WorkerCache:
    ''' The worker cache looks as follows:
        "workers: {
            "primary_name": {
                "0": {
                    "worker_name": "worker_name"
                    "worker_address": x.x.x.x:x,
                    "transactions": x.x.x.x:x
                },
                ...
            }
            ...
        },
    '''

    def __init__(self, workers, base_port):
        ''' The `workers` field looks as follows:
            {
                "primary_name": {
                    worker_name: ["host", "host", ...]
                    },
                ...
            }
        '''
        assert isinstance(workers, OrderedDict)
        assert all(isinstance(x, str) for x in workers.keys())
        assert all(
            isinstance(x, OrderedDict) for x in workers.values()
        )
        assert all(isinstance(x, str)
                   for y in workers.values() for x in y.keys())
        assert all(isinstance(x, list) and len(x) >=
                   1 for y in workers.values() for x in y.values())
        assert all(
            isinstance(x, str) for z in workers.values() for y in z.values() for x in y
        )
        assert len({len(x) for y in workers.values()
                   for x in y.values()}) == 1
        assert isinstance(base_port, int) and base_port > 1024

        port = base_port
        self.json = {'workers': OrderedDict(), 'epoch': 0}
        for primary_name, worker_info in workers.items():
            for worker_key, hosts in worker_info.items():
                workers_addr = OrderedDict()
                for j, host in enumerate(hosts):
                    workers_addr[j] = {
                        'name': worker_key,
                        'worker_address': f'/ip4/{host}/udp/{port}',
                        'transactions': f'/ip4/{host}/tcp/{port + 1}/http',
                    }
                    port += 2
                self.json['workers'][primary_name] = workers_addr

    def workers_addresses(self, faults=0):
        ''' Returns an ordered list of list of workers' addresses. '''
        assert faults < self.size()
        addresses = []
        good_nodes = self.size() - faults
        for worker_index in list(self.json['workers'].values())[:good_nodes]:
            worker_addresses = []
            for id, worker in worker_index.items():
                worker_addresses += [(id,
                                      multiaddr_to_url_data(worker['transactions']))]
            addresses.append(worker_addresses)
        return addresses

    def workers(self):
        ''' Returns the total number of workers (all authorities altogether). '''
        return sum(len(x.keys()) for x in self.json['workers'].values())

    def size(self):
        ''' Returns the number of workers. '''
        return len(self.json['workers'])

    def remove_nodes(self, nodes):
        ''' remove the `nodes` last nodes from the worker cache. '''
        assert nodes < self.size()
        for _ in range(nodes):
            self.json['workers'].popitem()

    def ips(self, name=None):
        ''' Returns all the ips associated with an workers (in any order). '''
        if name is None:
            names = list(self.json['workers'].keys())
        else:
            names = [name]

        ips = set()
        for name in names:
            for worker in self.json['workers'][name].values():
                ips.add(self.ip(worker['worker_address']))
                ips.add(self.ip(worker['transactions']))
        return ips

    def print(self, filename):
        assert isinstance(filename, str)
        with open(filename, 'w') as f:
            dump(self.json, f, indent=4, sort_keys=True)

    @staticmethod
    def ip(multi_address):
        address = multiaddr_to_url_data(multi_address)
        assert isinstance(address, str)
        return address.split(':')[1].strip("/")


class LocalWorkerCache(WorkerCache):
    def __init__(self, primary_names, worker_names, port, workers):
        assert isinstance(primary_names, list)
        assert all(isinstance(x, str) for x in primary_names)
        assert isinstance(worker_names, list)
        assert all(isinstance(x, str) for x in worker_names)
        assert isinstance(port, int)
        assert isinstance(workers, int) and workers > 0
        workers = OrderedDict(
            (x, OrderedDict(
                (worker_names[i*workers + y], ['127.0.0.1']*workers) for y in range(workers))
             ) for i, x in enumerate(primary_names))
        super().__init__(workers, port)


class Committee:
    ''' The committee looks as follows:
        "authorities: {
            "name": {
                "stake": 1,
                "primary_address": x.x.x.x:x,
                "network_key: NETWORK_KEY==
            },
            ...
        }
    '''

    def __init__(self, addresses, base_port):
        ''' The `addresses` field looks as follows:
            {
                "name": ["host", "host", ...],
                ...
            }
        '''
        assert isinstance(addresses, OrderedDict)
        assert all(isinstance(x, str) for x in addresses.keys())
        assert all(
            isinstance(address, list) and len(address) >= 1 for (_, address) in addresses.values()
        )
        assert all(
            isinstance(x, str) for (_, address) in addresses.values() for x in address
        )
        assert len({len(x) for x in addresses.values()}) == 1
        assert isinstance(base_port, int) and base_port > 1024

        port = base_port
        self.json = {'authorities': OrderedDict(), 'epoch': 0}
        for name, (network_name, hosts) in addresses.items():
            host = hosts.pop(0)
            primary_addr = f'/ip4/{host}/udp/{port}'
            port += 1

            self.json['authorities'][name] = {
                'stake': 1,
                'protocol_key': name,
                'protocol_key_bytes': name,
                'primary_address': primary_addr,
                'network_key': network_name,
                'hostname': host
            }

    def primary_addresses(self, faults=0):
        ''' Returns an ordered list of primaries' addresses. '''
        assert faults < self.size()
        addresses = []
        good_nodes = self.size() - faults
        for authority in list(self.json['authorities'].values())[:good_nodes]:
            addresses += [multiaddr_to_url_data(authority['primary_address'])]
        return addresses

    def ips(self, name=None):
        ''' Returns all the ips associated with an authority (in any order). '''
        if name is None:
            names = list(self.json['authorities'].keys())
        else:
            names = [name]

        ips = set()
        for name in names:
            ips.add(self.ip(self.json['authorities'][name]['primary_address']))
        return ips

    def remove_nodes(self, nodes):
        ''' remove the `nodes` last nodes from the committee. '''
        assert nodes < self.size()
        for _ in range(nodes):
            self.json['authorities'].popitem()

    def size(self):
        ''' Returns the number of authorities. '''
        return len(self.json['authorities'])

    def print(self, filename):
        assert isinstance(filename, str)
        with open(filename, 'w') as f:
            dump(self.json, f, indent=4, sort_keys=True)

    @staticmethod
    def ip(multi_address):
        address = multiaddr_to_url_data(multi_address)
        assert isinstance(address, str)
        return address.split(':')[1].strip("/")


class LocalCommittee(Committee):
    def __init__(self, names, network_names, port):
        assert isinstance(names, list)
        assert all(isinstance(x, str) for x in names)
        assert isinstance(port, int)
        assert len(names) == len(network_names)
        addresses = OrderedDict((name, (network_name, [
                                '127.0.0.1'])) for name, network_name in zip(names, network_names))
        super().__init__(addresses, port)


class NodeParameters:
    def __init__(self, json):
        inputs = []
        try:
            inputs += [json['header_num_of_batches_threshold']]
            inputs += [json['max_header_num_of_batches']]
            inputs += [json['max_header_delay']]
            inputs += [json['gc_depth']]
            inputs += [json['sync_retry_delay']]
            inputs += [json['sync_retry_nodes']]
            inputs += [json['batch_size']]
            inputs += [json['max_batch_delay']]
            inputs += [json['max_concurrent_requests']]
        except KeyError as e:
            raise ConfigError(f'Malformed parameters: missing key {e}')

        self.json = json

    def print(self, filename):
        assert isinstance(filename, str)
        with open(filename, 'w') as f:
            dump(self.json, f, indent=4, sort_keys=True)


class BenchParameters:
    def __init__(self, json):
        try:
            self.faults = int(json['faults'])

            nodes = json['nodes']
            nodes = nodes if isinstance(nodes, list) else [nodes]
            if not nodes or any(x <= 1 for x in nodes):
                raise ConfigError('Missing or invalid number of nodes')
            self.nodes = [int(x) for x in nodes]

            rate = json['rate']
            rate = rate if isinstance(rate, list) else [rate]
            if not rate:
                raise ConfigError('Missing input rate')
            self.rate = [int(x) for x in rate]

            self.workers = int(json['workers'])

            if 'collocate' in json:
                self.collocate = bool(json['collocate'])
            else:
                self.collocate = True

            self.tx_size = int(json['tx_size'])

            self.duration = int(json['duration'])

            if 'failpoints' in json:
                self.failpoints = bool(json['failpoints'])
            else:
                self.failpoints = False

            self.runs = int(json['runs']) if 'runs' in json else 1
        except KeyError as e:
            raise ConfigError(f'Malformed bench parameters: missing key {e}')

        except ValueError:
            raise ConfigError('Invalid parameters type')

        if min(self.nodes) <= self.faults:
            raise ConfigError('There should be more nodes than faults')


class PlotParameters:
    def __init__(self, json):
        try:
            faults = json['faults']
            faults = faults if isinstance(faults, list) else [faults]
            self.faults = [int(x) for x in faults] if faults else [0]

            nodes = json['nodes']
            nodes = nodes if isinstance(nodes, list) else [nodes]
            if not nodes:
                raise ConfigError('Missing number of nodes')
            self.nodes = [int(x) for x in nodes]

            workers = json['workers']
            workers = workers if isinstance(workers, list) else [workers]
            if not workers:
                raise ConfigError('Missing number of workers')
            self.workers = [int(x) for x in workers]

            if 'collocate' in json:
                self.collocate = bool(json['collocate'])
            else:
                self.collocate = True

            self.tx_size = int(json['tx_size'])

            max_lat = json['max_latency']
            max_lat = max_lat if isinstance(max_lat, list) else [max_lat]
            if not max_lat:
                raise ConfigError('Missing max latency')
            self.max_latency = [int(x) for x in max_lat]

        except KeyError as e:
            raise ConfigError(f'Malformed bench parameters: missing key {e}')

        except ValueError:
            raise ConfigError('Invalid parameters type')

        if len(self.nodes) > 1 and len(self.workers) > 1:
            raise ConfigError(
                'Either the "nodes" or the "workers can be a list (not both)'
            )

    def scalability(self):
        return len(self.workers) > 1
