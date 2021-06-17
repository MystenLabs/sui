from log import LogParser
from committee import CommitteeParser
from aggregator import LogAggregator as NewAggregator
from plot import Ploter

import boto3
from fabric import task, Connection, ThreadingGroup as Group
from paramiko import RSAKey
from json import load, dump
from os import system
from os.path import join
from time import sleep
from re import findall
from math import ceil
from glob import glob
from re import findall
from sys import exit


SETUP_SCRIPT = 'aws-setup.sh'

# --- Start Config ---
REPO = 'mempool-research'
BRANCH = 'block_explorer'
LOCAL_BINARY_PATH = '../rust/target/release/'
BINARY = 'bench_worker'
LOCAL_BINARY = join(LOCAL_BINARY_PATH, BINARY)
PORT = 8080
COMMITTEE_CONFIG_FILE = 'committee_config.json'
LOCAL_COMMITTEE_CONFIG_FILE = 'local_committee_config.json'
VERBOSE = False  # Activate verbose logging.
REGIONS = [
    'us-east-1', 'eu-north-1', 'ap-southeast-2', 'us-west-1', 'ap-northeast-1'
]


def run_node_command(i, batch_size, local=False, single_machine=False):
    ''' Returns the command to run a node. '''
    # NOTE: Calling tmux in threaded groups does not work.
    binary = LOCAL_BINARY if local else BINARY
    committee = LOCAL_COMMITTEE_CONFIG_FILE if local else COMMITTEE_CONFIG_FILE
    pipe = '2>' if local else '|& tee'  # '2>' does not seem to work on AWS...
    verbose = '-v' if VERBOSE else ''
    run_all = '--all' if local or single_machine else ''
    command = (
        f'./{binary} {verbose} benchmark node --batch {batch_size} '
        f'--node node_config_{i}.json --committee {committee} {run_all}'
    )
    return f'tmux new -d -s "node{i}" "{command} {pipe} node-{i}.log"'


def run_client_command(i, txs_size, host, rate, others, local=False):
    ''' Returns the command to run a benchmark client. '''
    # NOTE: Calling tmux in threaded groups does not work.
    binary = LOCAL_BINARY if local else BINARY
    pipe = '2>' if local else '|& tee'  # '2>' does not seem to work on AWS...
    verbose = '-v' if VERBOSE else ''
    command = (
        f'./{binary} {verbose} benchmark client --size {txs_size} '
        f'--address {host} --rate {rate} --others {" ".join(others)}'
    )
    return f'tmux new -d -s "client{i}" "{command} {pipe} client-{i}.log"'


def credentials():
    ''' Set the username and path to key file. '''
    return {
        'user': 'ubuntu',
        'keyfile': '/Users/asonnino/.ssh/aws-fb.pem'
    }


def large_filter(instance):
    ''' Specify a filter to select only the desired hosts. '''
    name = next(tag['Value'] for tag in instance.tags if 'Name' in tag['Key'])
    return 'dag-node'.casefold() == name.casefold()


def medium_filter(instance):
    ''' Specify a filter to select only the desired hosts. '''
    name = next(tag['Value'] for tag in instance.tags if 'Name' in tag['Key'])
    return 'dag-node-wan'.casefold() == name.casefold()

# --- End Config ---


FILTER = large_filter


def set_hosts(ctx, cred=credentials):
    ''' Set the credentials, hosts, and a list of instances into context. '''
    if ctx.connect_kwargs:
        return

    # Set credentials into the context.
    credentials = cred()
    ctx.user = credentials['user']
    ctx.keyfile = credentials['keyfile']  # This is only used for `fab info`.
    ctx.connect_kwargs.pkey = RSAKey.from_private_key_file(
        credentials['keyfile']
    )

    # Get all instances that match the input status and filter.
    ctx.hosts = {}
    for region in REGIONS:
        ec2resource = boto3.resource('ec2', region_name=region)
        instances = ec2resource.instances.filter(
            Filters=[{'Name': 'instance-state-name', 'Values': ['running']}]
        )
        ctx.hosts[region] = {
            x.public_ip_address for x in instances if FILTER(x)}


@task
def info(ctx):
    ''' Print commands to ssh into hosts (debug). '''
    set_hosts(ctx)
    total = sum(len(x) for x in ctx.hosts.values())
    print(f'\nAvailable machines ({total}):')
    for region, hosts in ctx.hosts.items():
        if not hosts:
            continue
        print(f'Region: {region.upper()}')
        for i, host in enumerate(hosts):
            new_line = '\n' if (i+1) % 5 == 0 else ''
            print(f'{i}\t ssh -i {ctx.keyfile} {ctx.user}@{host} {new_line}')
        print()


@task
def start(ctx, max=4):
    ''' Start at most 'max' instances per data center. '''
    count = 0
    for region in REGIONS:
        ec2resource = boto3.resource('ec2', region_name=region)
        instances = ec2resource.instances.filter(
            Filters=[{'Name': 'instance-state-name', 'Values': ['stopped']}]
        )
        ids = [x.id for x in instances if FILTER(x)]
        ids = ids[:max]
        count += len(ids)
        if ids:
            ec2 = boto3.client('ec2', region_name=region)
            _ = ec2.start_instances(InstanceIds=ids, DryRun=False)
    print(f'Starting {count} machines.')


@task
def stop(ctx):
    ''' Stop all instances. '''
    count = 0
    for region in REGIONS:
        ec2resource = boto3.resource('ec2', region_name=region)
        instances = ec2resource.instances.filter(
            Filters=[{'Name': 'instance-state-name', 'Values': ['running']}]
        )
        ids = [x.id for x in instances if FILTER(x)]
        count += len(ids)
        if ids:
            ec2 = boto3.client('ec2', region_name=region)
            _ = ec2.stop_instances(InstanceIds=ids, DryRun=False)
    print(f'Stopping {count} machines.')


@task
def reboot(ctx):
    ''' Reboot all instances. '''
    count = 0
    for region in REGIONS:
        ec2resource = boto3.resource('ec2', region_name=region)
        instances = ec2resource.instances.filter(
            Filters=[{'Name': 'instance-state-name', 'Values': ['running']}]
        )
        ids = [x.id for x in instances if FILTER(x)]
        count += len(ids)
        if ids:
            ec2 = boto3.client('ec2', region_name=region)
            ec2.reboot_instances(InstanceIds=ids, DryRun=False)
    print(f'Rebooting {count} machines.')


@task
def keys(ctx):
    ''' Upload a specific key to each instance. '''
    key_file = '/Users/asonnino/.ssh/dag'
    set_hosts(ctx)
    hosts = frozenset().union(*ctx.hosts.values())
    for i, host in enumerate(hosts):
        print(f'[{i+1}/{len(hosts)}] Uploading GitHub keys...', end='\r')
        c = Connection(host, user=ctx.user, connect_kwargs=ctx.connect_kwargs)
        c.run('mkdir -p .ssh')
        c.put(key_file, '.ssh/id_rsa')
        c.put(f'{key_file}.pub', '.ssh/id_rsa.pub')


@task
def install(ctx, cleanup=False):
    ''' Install the repo and dependencies on all hosts. '''
    set_hosts(ctx)
    hosts = frozenset().union(*ctx.hosts.values())
    for i, host in enumerate(hosts):
        print(f'[{i+1}/{len(hosts)}] Uploading script...', end='\r')
        if cleanup:
            c.run('rm -rf *', hide=True)
        c = Connection(host, user=ctx.user, connect_kwargs=ctx.connect_kwargs)
        c.put(SETUP_SCRIPT, '.')

    print('Setting up all hosts...            ')
    g = Group(*hosts, user=ctx.user, connect_kwargs=ctx.connect_kwargs)
    g.run(f'chmod +x {SETUP_SCRIPT} && ./{SETUP_SCRIPT}', hide=True)
    print('All hosts are ready.')


@task
def update(ctx):
    ''' Update all machines from GitHub. '''
    set_hosts(ctx)
    hosts = frozenset().union(*ctx.hosts.values())
    command = (
        f'rm -f {BINARY} ; cd {REPO}/rust && git fetch && '
        f'git checkout {BRANCH} && git pull && source $HOME/.cargo/env && '
        f'cargo build --release && cp target/release/{BINARY} ~/{BINARY}'
    )
    print(f'Updating {len(hosts)} machines.')
    g = Group(*hosts, user=ctx.user, connect_kwargs=ctx.connect_kwargs)
    g.run(command, hide=True)
    print('All machines are successfully updated.')


@task
def config(ctx, nodes=10, workers=1, single_machine=True):
    ''' Configure nodes and workers on the same machine.'''
    set_hosts(ctx)

    # Instantiate the primary and all workers on the same machine.
    # Select the hosts in different data centers (as much as possible).
    if single_machine:
        hosts = zip(*ctx.hosts.values())
        hosts = [x for y in hosts for x in y]
        if len(hosts) < nodes:
            print(f'Not enough instances: there should be at least {nodes}')
            exit(1)

    # Instantiate the primary and each of its workers on a separate machine.
    # Each validator is instantiated in a separate datacenter.
    else:
        hosts = []
        for ips in ctx.hosts.values():
            if len(ips) >= workers+1:
                hosts += list(ips)[:workers+1]
        if len(hosts) < nodes * (workers+1):
            print(
                'Not enough instances: '
                f'there should be at least {nodes} data centers with '
                f'{workers} instances each'
            )
            exit(1)

    # Generate the configuration files.
    system('rm *.json >/dev/null 2>&1')
    system(f'(cd {LOCAL_BINARY_PATH} && cargo build --release)')
    system(f'./{LOCAL_BINARY} generate --nodes {nodes} --workers {workers}')

    # Update hosts and ports in the committee file.
    committee = CommitteeParser(LOCAL_COMMITTEE_CONFIG_FILE)
    names_to_hosts = {}
    hosts = iter(hosts)
    for p, w in committee.names().items():
        primary_host = next(hosts)
        names_to_hosts[p] = (primary_host, PORT)
        for i, worker in enumerate(w):
            worker_host = primary_host if single_machine else next(hosts)
            names_to_hosts[worker] = (worker_host, PORT + i + 1)

    committee.update_hosts(names_to_hosts)
    with open(COMMITTEE_CONFIG_FILE, 'w') as f:
        dump(committee.to_json(), f, indent=4)

    # Cleanup config files from hosts.
    print('Cleaning up old config files from hosts...')
    to_clean = [x[0] for x in names_to_hosts.values()]
    g = Group(*to_clean, user=ctx.user, connect_kwargs=ctx.connect_kwargs)
    g.run('rm *.json || true', hide=True)

    # Upload the configuration files to each machine.
    for i in range(len(names_to_hosts)):
        print(f' [{i+1}/{len(names_to_hosts)}] Uploading config files...', end='\r')
        filename = f'node_config_{i}.json'
        with open(filename, 'r') as f:
            name = load(f)['id']
        host = names_to_hosts[name][0]
        c = Connection(host, user=ctx.user, connect_kwargs=ctx.connect_kwargs)
        c.put(COMMITTEE_CONFIG_FILE, '.')
        c.put(filename, '.')

    print(f'Configured {nodes} primaries with {workers} worker(s) each.')


@task
def kill(ctx, hide=True, cleanup=False):
    ''' Kill the process on all machines and (optionally) delete all logs. '''
    set_hosts(ctx)
    hosts = frozenset().union(*ctx.hosts.values())
    remove_logs = ' rm -rf .storage_* ; rm *.log' if cleanup else 'true'
    command = f'tmux kill-server ; {remove_logs}'
    g = Group(*hosts, user=ctx.user, connect_kwargs=ctx.connect_kwargs)
    g.run(f'{command} || true', hide=hide)
    if not hide:
        print('All machines are cleaned up.')


@task
def logs(ctx, hide=False, faults=0):
    ''' Download the log files from every host. '''
    set_hosts(ctx)
    committee = CommitteeParser(COMMITTEE_CONFIG_FILE)
    committee.remove_faulty(faults)
    hosts = set(committee.all_hosts())
    system('rm *.log >/dev/null 2>&1')

    # Keep only selected lines to compress node logs.
    keep = [
        'Committee size', 'Number of workers per node', 'Txs batch size',
        'certified digest', 'Commit digest', 'Making header', 'warn ', 'panic',
        'Received a tx digest'#, 'CERT'
    ]

    # Download log files.
    for i, host in enumerate(hosts):
        print(f' [{i+1}/{len(hosts)}] Downloading logs...', end='\r')
        c = Connection(host, user=ctx.user, connect_kwargs=ctx.connect_kwargs)
        output = str(c.run('ls *.log', hide=True))

        for i in findall(r'client-(\d+).log', output):
            c.get(f'client-{i}.log')

        for i in findall(r'node-(\d+).log', output):
            log, out = f'node-{i}.log', f'node-{i}.compressed.log'
            command = [f'(cat {log} | grep "{x}" >> {out})' for x in keep]
            command = ' ; '.join(command)
            c.run(f'rm {out} || true', hide=True)
            c.run(f'{command} || true', hide=True)
            c.get(out)

    # Parse logs.
    clients_logs, nodes_logs = [], []
    for logfile in sorted(glob('client-*.log')):
        with open(logfile, 'r') as f:
            clients_logs += [f.read()]
    for logfile in sorted(glob('node-*.log')):
        with open(logfile, 'r') as f:
            nodes_logs += [f.read()]

    print('Finished downloading logs.')
    parser = LogParser(clients_logs, nodes_logs, nodes_logs, faults=faults)
    if not hide:
        print(parser.results)
    print('Finished parsing logs.')
    return parser


@task
def tps(ctx, txs_size=512, batch_size=10**4, rate=100_000, delay=300,
        share_load=True, hide=True, single_machine=True, faults=0):
    ''' Run a tps benchmark on AWS. '''
    set_hosts(ctx)
    kill(ctx, hide=True, cleanup=True)
    committee = CommitteeParser(COMMITTEE_CONFIG_FILE)
    n = committee.size()

    # Do not boot faulty nodes.
    committee.remove_faulty(faults)

    # Run clients.
    # The clients will wait for all workers to be ready.
    print('Running clients...')
    hosts, ports = committee.workers_hosts(), committee.workers_ports()
    addresses = [f'{x}:{y}' for x, y in zip(hosts, ports)]
    rate_share = ceil(rate / n) if share_load else rate
    for i, (host, address) in enumerate(zip(hosts, addresses)):
        c = Connection(host, user=ctx.user, connect_kwargs=ctx.connect_kwargs)
        client_command = run_client_command(
            i, txs_size, address, rate_share, addresses
        )
        c.run(client_command, hide=True)

    # Run the primaries.
    print('Running nodes...' if single_machine else 'Running primaries...')
    i = 0
    for host in committee.primaries_hosts():
        c = Connection(host, user=ctx.user, connect_kwargs=ctx.connect_kwargs)
        primary_command = run_node_command(
            i, batch_size, single_machine=single_machine
        )
        c.run(primary_command, hide=True)
        i += 1

    # Run workers.
    if not single_machine:
        print('Running workers...')
        hosts = committee.workers_hosts()
        for host in hosts:
            c = Connection(host, user=ctx.user,
                           connect_kwargs=ctx.connect_kwargs)
            worker_command = run_node_command(
                i, batch_size, single_machine=single_machine
            )
            c.run(worker_command, hide=True)
            i += 1

    print(f'Running benchmark ({delay} sec)...')
    sleep(delay)  # Wait for the nodes to process all txs.
    print('Killing testbed...')
    kill(ctx, hide=True, cleanup=False)

    # Download log files and print results.
    parser = logs(ctx, hide=True, faults=faults)
    if not hide:
        print(parser.results)
    return parser


@task
def local(ctx, txs_size=512, batch_size=1_000, rate=30_000, nodes=10,
          workers=1, delay=30, share_load=True, hide=False, faults=3):
    ''' Run a tps benchmark on the local host. '''
    print(f'Running benchmark (nodes={nodes}, workers={workers})')
    system(f'(cd {LOCAL_BINARY_PATH} && cargo build --release)')
    system('rm *.json *.log >/dev/null 2>&1')
    system('rm .storage_* >/dev/null 2>&1')
    system('tmux kill-server >/dev/null 2>&1')


    # Generate all configuration files.
    system(f'./{LOCAL_BINARY} generate --nodes {nodes} --workers {workers}')

    # Run nodes.
    for i in range(nodes):
        cmd_name = run_node_command(i, batch_size, local=True)
        # print(cmd_name)
        system(cmd_name)
    sleep(2)  # Wait for the nodes to connect with each other.

    # Run clients.
    committee = CommitteeParser(LOCAL_COMMITTEE_CONFIG_FILE)
    committee.remove_faulty(faults)
    ports = committee.workers_ports()
    addresses = [f'127.0.0.1:{x}' for x in ports]
    rate_share = ceil(rate / len(addresses))
    for i, addr in enumerate(addresses):
        command = run_client_command(
            i, txs_size, addr, rate_share, addresses, local=True
        )
        # print(command)
        system(command)

    sleep(delay)  # Wait for the nodes to process all txs.
    system('tmux kill-server >/dev/null 2>&1')

    # Process logs and print result.
    clients_logs, nodes_logs = [], []
    for logfile in sorted(glob('client-*.log')):
        with open(logfile, 'r') as f:
            clients_logs += [f.read()]
    for logfile in sorted(glob('node-*.log')):
        with open(logfile, 'r') as f:
            nodes_logs += [f.read()]

    parser = LogParser(clients_logs, nodes_logs, nodes_logs, faults=faults)
    if not hide:
        print(parser.results)
    return parser


@task
def stats(ctx, hide=False):
    # Process logs and print result.
    clients_logs, nodes_logs = [], []
    for logfile in sorted(glob('client-*.log')):
        with open(logfile, 'r') as f:
            clients_logs += [f.read()]
    for logfile in sorted(glob('node-*.log')):
        with open(logfile, 'r') as f:
            nodes_logs += [f.read()]

    parser = LogParser(clients_logs, nodes_logs, nodes_logs)
    if not hide:
        print(parser.results)
    return parser


@task
def bench(ctx):
    ''' Run multiple TPS benchmarks on AWS. '''
    # --- start config ---
    runs = 1
    delay = 300
    share_load = True
    single_machine = False

    # Exactly one of the parameters below must be a list.
    nodes, workers = 4, 4
    faults = 0
    txs_size, batch_size, rate = 512, 1_000, [350_000]
    # --- end config ---

    params = {
        'txs_size': txs_size,
        'batch_size': batch_size,
        'rate': rate,
        'nodes': nodes,
        'workers': workers,
        'faults': faults,
        'delay': delay,
        'share_load': share_load,
        'single_machine': single_machine,
        'hide': True
    }

    # Run benchmarks (this may take a long time).
    update(ctx)
    print(f'Running benchmarks ({runs} runs per measure)')
    system('rm *.json *.log >/dev/null 2>&1')
    target = next(k for k, v in params.items() if isinstance(v, list))
    for x in params[target]:
        print()
        clone_params = params.copy()
        clone_params[target] = x

        n, w = clone_params.pop('nodes'), clone_params.pop('workers')
        mode = clone_params['single_machine']
        config(ctx, nodes=n, workers=w, single_machine=mode)

        rate = clone_params['rate']
        clone_params['rate'] //= w 
        for _ in range(runs):
            parser = tps(ctx, **clone_params)
            with open(f'results/scaling/benchmark.dag.{n}.{w}.{rate}.{faults}.txt', 'a') as f:
                f.write(f'Branch: {BRANCH}\n')
                f.write(parser.config)
                f.write(parser.dag_results)
                f.write('\n')
            with open(f'results/scaling/benchmark.consensus.{n}.{w}.{rate}.{faults}.txt', 'a') as f:
                f.write(f'Branch: {BRANCH}\n')
                f.write(parser.config)
                f.write(parser.consensus_results)
                f.write('\n')


@task
def aggregate(ctx):
    files = glob('results/scaling/benchmark.consensus.*.txt')
    NewAggregator(files).print()


@task
def plot(ctx):
    results = []
    for filename in glob('plot/*.txt'):
        with open(filename, 'r') as f:
            results += [f.read()]

    ploter = Ploter(results)
    #ploter.plot_tps('Workers per validator', ploter.max_latency)
    ploter.plot_client_latency(ploter.workers)
    #ploter.plot_robustness(ploter.nodes)
