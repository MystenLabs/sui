# Copyright(C) Facebook, Inc. and its affiliates.
from fabric import task
from benchmark.seed import SeedData

from benchmark.local import LocalBench
from benchmark.full_demo import Demo

from benchmark.logs import ParseError, LogParser
from benchmark.utils import Print
from benchmark.plot import Ploter, PlotError
from benchmark.instance import InstanceManager
from benchmark.remote import Bench, BenchError


@task
def local(ctx, debug=True):
    ''' Run benchmarks on localhost '''
    bench_params = {
        'faults': 0,
        'nodes': 4,
        'workers': 1,
        'rate': 50_000,
        'tx_size': 512,
        'duration': 20,
        'mem_profiling': False
    }
    node_params = {
        'header_size': 1_000,  # bytes
        'max_header_delay': '200ms',  # ms
        'gc_depth': 50,  # rounds
        'sync_retry_delay': '10_000ms',  # ms
        'sync_retry_nodes': 3,  # number of nodes
        'batch_size': 500_000,  # bytes
        'max_batch_delay': '200ms',  # ms,
        'block_synchronizer': {
            'certificates_synchronize_timeout': '2_000ms',
            'payload_synchronize_timeout': '2_000ms',
            'payload_availability_timeout': '2_000ms',
            'handler_certificate_deliver_timeout': '2_000ms'
        },
        "consensus_api_grpc": {
            "socket_addr": "/ip4/127.0.0.1/tcp/0/http",
            "get_collections_timeout": "5_000ms",
            "remove_collections_timeout": "5_000ms"
        },
        'max_concurrent_requests': 500_000,
        'prometheus_metrics': {
            "socket_addr": "127.0.0.1:0"
        }
    }
    try:
        ret = LocalBench(bench_params, node_params).run(debug)
        print(ret.result())
    except BenchError as e:
        Print.error(e)


@task
def demo(ctx, debug=True):
    ''' Run benchmarks on localhost '''
    bench_params = {
        'faults': 0,
        'nodes': 4,
        'workers': 1,
        'rate': 50_000,
        'tx_size': 512,
        'duration': 10,
    }
    node_params = {
        "batch_size": 500000,
        "block_synchronizer": {
            "certificates_synchronize_timeout": "2_000ms",
            "handler_certificate_deliver_timeout": "2_000ms",
            "payload_availability_timeout": "2_000ms",
            "payload_synchronize_timeout": "2_000ms"
        },
        "consensus_api_grpc": {
            "get_collections_timeout": "5_000ms",
            "remove_collections_timeout": "5_000ms",
            "socket_addr": "/ip4/0.0.0.0/tcp/0/http"
        },
        "gc_depth": 50,  # rounds
        "header_size": 1000,  # bytes
        "max_batch_delay": "200ms",  # ms
        "max_concurrent_requests": 500_000,
        "max_header_delay": "2000ms",  # ms
        "sync_retry_delay": "10_000ms",  # ms
        "sync_retry_nodes": 3,  # number of nodes
        'prometheus_metrics': {
            "socket_addr": "127.0.0.1:0"
        }
    }
    try:
        ret = Demo(bench_params, node_params).run(debug)
        print(ret.result())
    except BenchError as e:
        Print.error(e)


@task
def seed(ctx, starting_data_port):
    ''' Run data seeder '''
    bench_params = {
        'faults': 0,
        'nodes': 4,
        'workers': 1,
        'rate': 50_000,
        'tx_size': 512,
        'duration': 20,
    }
    try:
        SeedData(bench_params).run(int(starting_data_port))
    except BenchError as e:
        Print.error(e)


@task
def create(ctx, nodes=2):
    ''' Create a testbed'''
    try:
        InstanceManager.make().create_instances(nodes)
    except BenchError as e:
        Print.error(e)


@task
def destroy(ctx):
    ''' Destroy the testbed '''
    try:
        InstanceManager.make().terminate_instances()
    except BenchError as e:
        Print.error(e)


@task
def start(ctx, max=2):
    ''' Start at most `max` machines per data center '''
    try:
        InstanceManager.make().start_instances(max)
    except BenchError as e:
        Print.error(e)


@task
def stop(ctx):
    ''' Stop all machines '''
    try:
        InstanceManager.make().stop_instances()
    except BenchError as e:
        Print.error(e)


@task
def info(ctx):
    ''' Display connect information about all the available machines '''
    try:
        InstanceManager.make().print_info()
    except BenchError as e:
        Print.error(e)


@task
def install(ctx):
    ''' Install the codebase on all machines '''
    try:
        Bench(ctx).install()
    except BenchError as e:
        Print.error(e)


@task
def remote(ctx, debug=False):
    ''' Run benchmarks on AWS '''
    bench_params = {
        'faults': 3,
        'nodes': [10],
        'workers': 1,
        'collocate': True,
        'rate': [10_000, 110_000],
        'tx_size': 512,
        'duration': 300,
        'runs': 2,
    }
    node_params = {
        'header_size': 1_000,  # bytes
        'max_header_delay': '200ms',  # ms
        'gc_depth': 50,  # rounds
        'sync_retry_delay': '10_000ms',  # ms
        'sync_retry_nodes': 3,  # number of nodes
        'batch_size': 500_000,  # bytes
        'block_synchronizer': {
            'certificates_synchronize_timeout': '2_000ms',
            'payload_synchronize_timeout': '2_000ms',
            'payload_availability_timeout': '2_000ms',
            'handler_certificate_deliver_timeout': '2_000ms'
        },
        "consensus_api_grpc": {
            "socket_addr": "/ip4/127.0.0.1/tcp/0/http",
            "get_collections_timeout": "5_000ms",
            "remove_collections_timeout": "5_000ms"
        },
        'max_concurrent_requests': 500_000,
        'prometheus_metrics': {
            "socket_addr": "127.0.0.1:0"
        }
    }
    try:
        Bench(ctx).run(bench_params, node_params, debug)
    except BenchError as e:
        Print.error(e)


@task
def plot(ctx):
    ''' Plot performance using the logs generated by "fab remote" '''
    plot_params = {
        'faults': [0],
        'nodes': [10, 20, 50],
        'workers': [1],
        'collocate': True,
        'tx_size': 512,
        'max_latency': [3_500, 4_500]
    }
    try:
        Ploter.plot(plot_params)
    except PlotError as e:
        Print.error(BenchError('Failed to plot performance', e))


@task
def kill(ctx):
    ''' Stop execution on all machines '''
    try:
        Bench(ctx).kill()
    except BenchError as e:
        Print.error(e)


@task
def logs(ctx):
    ''' Print a summary of the logs '''
    try:
        print(LogParser.process('./logs', faults='?').result())
    except ParseError as e:
        Print.error(BenchError('Failed to parse logs', e))
