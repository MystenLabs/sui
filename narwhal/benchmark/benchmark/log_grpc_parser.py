# Copyright(C) Facebook, Inc. and its affiliates.
from datetime import datetime
from dateutil import parser
from glob import glob
from logging import exception
from multiprocessing import Pool
from os.path import join
from re import findall, search
from statistics import mean


from benchmark.utils import Print


class ParseError(Exception):
    pass


class LogGrpcParser:
    def __init__(self, primaries, faults=0):
        assert all(isinstance(x, str) for x in primaries)
        self.faults = faults

        # Parse the primaries logs.
        try:
            with Pool() as p:
                results = p.map(self._parse_primaries, primaries)
        except (ValueError, IndexError, AttributeError) as e:
            exception(e)
            raise ParseError(f'Failed to parse nodes\' logs: {e}')
        self.grpc_ips = results
        for ip in self.grpc_ips:
            print(f'Found port for grpc server at {ip}')

    def _merge_results(self, input):
        # Keep the earliest timestamp.
        merged = {}
        for x in input:
            for k, v in x:
                if k not in merged or merged[k] > v:
                    merged[k] = v
        return merged

    def _parse_primaries(self, log):
        ip = search(
            r'Consensus API gRPC Server listening on /ip4/.+/tcp/(.+)/http', log).group(1)
        return ip

    @classmethod
    def process(cls, directory, faults=0):
        assert isinstance(directory, str)

        primaries = []
        for filename in sorted(glob(join(directory, 'primary-*.log'))):
            with open(filename, 'r') as f:
                primaries += [f.read()]

        return cls(primaries, faults=faults)
