# Copyright (c) Facebook Inc.
# SPDX-License-Identifier: Apache-2.0

"""
AWS AT FB
	Follow the instructions at https://fb.quip.com/YrDyAS3GDcw

INSTALL:
	1. create a virtual env:
		python -m virtualenv venv
		source venv/bin/activate

	2. install boto3 and fabric:
		pip install boto3
		pip install fabric

	3. configure aws:
		pip install awscli
		aws configure 	# input 'eu-north-1b' as region

CROSS-COMPILATION FOR UBUNTU:
	1. run the following commands
		brew install FiloSottile/musl-cross/musl-cross 	# warning: takes a long tome
		brew link musl-cross
		rustup target add x86_64-unknown-linux-musl

	2. add the following lines to a file called `.cargo/config` in the project dir:
		[target.x86_64-unknown-linux-musl]
		linker = "x86_64-linux-musl-gcc"

	3. build the executable
		CC_x86_64_unknown_linux_musl="x86_64-linux-musl-gcc" cargo build --release --target=x86_64-unknown-linux-musl

CREATE AWS INSTANCES:
	1. create a client instance, called `fastpay-client`
	2. create as many authority instances as required, called `fastpay-authorituy-#` (where # is a number)
	3. create a security group called `fastpay` and add all authorities to it
	4. open the following ports on the fastpay security group
		TCP 22
		UDP ALL

GET ACCESS TO EXISINT INSTANCES:
	1. got to the AWS console interface and generate a new .pem key file
	2. extract the public key from the .pem key file:
		ssh-keygen -f YOUR_KEY.pem -y > YOUR_KEY.pub
	3. send YOUR_KEY.pub to someone that has access to the machines so that they can add it to each machine:
		`./ssh/authorized_keys`

POSSIBLE ISSUES:
	- if boto3 outputs 'Request has expired', run 'source ~/bin/aws-mfa'. More info at https://fb.quip.com/YrDyAS3GDcwZ
	- try to relink musl-cross: brew unlink musl-cross && brew link musl-cross
	- if permission denied when cross compiling, run 'sudo cargo build --release --target=x86_64-unknown-linux-musl'
"""

from fabric.api import *
import boto3
from botocore.exceptions import ClientError
import os
import sys
import time

ec2 = boto3.client('ec2')

region = os.environ.get("AWS_EC2_REGION")
env.user = 'ubuntu'
env.key_filename = '~/.ssh/aws-fb.pem' 	# SET PATH TO KEY FILE HERE
linux_bin_dir = '../../target/x86_64-unknown-linux-musl/release/'
local_bin_dir = '../../target/release/'

"""
Initialise hosts, this command is run in conjunction with other commands.
COMMANDS:	fab set_hosts <COMMAND>
"""

instances_id = [] # automatically filled by 'set_hosts'
env.roledefs = {'client': [], 'authority': [], 'throughput': []} # automatically filled by 'set_hosts'

def set_hosts(value ='running', instance_type='fastpay', region=region):
	ec2resource = boto3.resource('ec2')
	instances = ec2resource.instances.filter(Filters=[{'Name': 'instance-state-name', 'Values': [value]}])
	for instance in instances:
		for tag in instance.tags or []:
			if 'Name'in tag['Key']:
				name = tag['Value']
		if instance_type in name:
			instances_id.append(instance.id)
			if name == 'fastpay-client':
				env.roledefs['client'].append(instance.public_ip_address)
			elif 'fastpay-authority' in name:
				env.roledefs['authority'].append(instance.public_ip_address)
			elif 'fastpay-throughput' in name:
				env.roledefs['throughput'].append(instance.public_ip_address)

"""
Start and stop instances.
COMMANDS:	fab start 				start all instances
			fab start:authority 	start all server
			fab start:client 		start all clients
			fab start:throughput 	start all throughput benchmark machines

			fab stop 				stop all instances
			fab stop:authority 		stop all server
			fab stop:client 		stop all clients
			fab stop:throughput 	stop all throughput benchmark machines
"""

def start(instance_type='fastpay'):
	set_hosts(value='stopped', instance_type=instance_type)
	try:
		ec2.start_instances(InstanceIds=instances_id, DryRun=True)
	except ClientError as e:
		if 'DryRunOperation' not in str(e):
			raise
	try:
		response = ec2.start_instances(InstanceIds=instances_id, DryRun=False)
		print(response)
	except ClientError as e:
		print(e)

def stop(instance_type='fastpay'):
	set_hosts(value='running', instance_type=instance_type)
	try:
		ec2.stop_instances(InstanceIds=instances_id, DryRun=True)
	except ClientError as e:
		if 'DryRunOperation' not in str(e):
			raise
	try:
		response = ec2.stop_instances(InstanceIds=instances_id, DryRun=False)
		print(response)
	except ClientError as e:
		print(e)

"""
Print commands to ssh into hosts (debug).
COMMANDS:	fab info
"""

def info():
	set_hosts()

	print('\nclients:')
	for client in env.roledefs['client']:
		print('\t ssh -i '+env.key_filename+' '+env.user+'@'+client)

	print('\nservers:')
	for server in env.roledefs['authority']:
		print('\t ssh -i '+env.key_filename+' '+env.user+'@'+server)

	print('\nthroughput:')
	for server in env.roledefs['throughput']:
		print('\t ssh -i '+env.key_filename+' '+env.user+'@'+server)


"""
Deploy the testnet.
COMMANDS: 	fab set_hosts deploy 			Update binary and reload same committee and initial accounts (but state is lost)
			fab set_hosts reset deploy 		Erase all files and create new committee and initial accounts

USE THE TESTNET:
	1. locate the file 'committee.json' specifying the set of FastPay authorities

	2. create a user account (it creates 'accounts.json'):
		./client --committee committee.json --accounts accounts.json create_accounts 1

	3. ask someone nice to transfer coins to your address

	4. read your balance:
		./client --committee committee.json --accounts accounts.json query_balance <YOUR_ADDRESS>'

	5. transfer coins:
		./client --committee committee.json --accounts accounts.json transfer --from <YOUR_ADDRESS> --to <RECIPIENT> <AMOUNT>
"""

committee_config = 'committee.json' # contains information about the committee
initial_accounts = 'initial_accounts.json' # hold addresses of the inital accounts
accounts = 'accounts.json' # hold all information about users accounts
base_port = 9500
num_shards = 15
num_initial_accounts = 10 # number of accounts to create intially
amount_initial_accounts = 10000 # amount to seed the initial accounts

def local_run(command):
	local('%s%s' % (local_bin_dir, command))

def cross_compile():
	local('(cd ../fastpay && \
		CC_x86_64_unknown_linux_musl="x86_64-linux-musl-gcc" \
		cargo build --release --target=x86_64-unknown-linux-musl)')

@roles('authority')
def clean():
	local('rm -f *.json')
	run('rm -r * || true')

def intitalize():
	# make committee
	for host in env.roledefs['authority']:
		with settings(host_string=host):
			command = './server --server %s.json generate --host %s --port %d --shards %d >> %s' \
				% (host, host, base_port, num_shards, committee_config)
			execute(local_run, command)

	# generate initial state
	command = './client --committee %s --accounts %s create_accounts %d >> %s' \
		% (committee_config, accounts, num_initial_accounts, initial_accounts)
	execute(local_run, command)


@roles('authority')
def upload_server_files():
	put(committee_config, '.')
	put(initial_accounts, '.')

	for host in env.roledefs['authority']:
		with settings(host_string=host):
			put(host+'.json', '.')

@roles('authority')
def run_server():
	run('tmux kill-server || true')
	run('rm -r server || true')
	put(linux_bin_dir+'server', '.')
	run('chmod +x *')
	run('for f in *.*.*.*; do mv -i "$f" "server.json" || true; done') # rename key file

	for i in range(num_shards):
		run('tmux new -d -s server-%d' % i)
		command = 'taskset\ --cpu-list\ %d\ ./server\ --server\ server.json\ run\ --initial_accounts\ %s\ --initial_balance\ %d\ --committee\ %s\ --shard\ %d' \
			% (i, initial_accounts, amount_initial_accounts, committee_config, i)
		run('tmux send -t server-%d.0 %s ENTER' % (i, command))

	'''
	run('tmux new -d -s server')
	command = './server\ --server\ server.json\ run\ --initial_accounts\ %s\ --initial_balance\ %d\ --committee\ %s' \
		% (initial_accounts, amount_initial_accounts, committee_config)
	run('tmux send -t server.0 %s ENTER' % command)
	'''

def reset():
	execute(cross_compile)
	execute(clean)
	execute(intitalize)
	execute(upload_server_files)

def deploy():
	execute(run_server)

"""
Run client to test latency (once the testnet is deployed)
COMMANDS:	fab set_hosts get_balance
			fab set_hosts transfer
			fab set_hosts mass_transfer
			fab set_hosts quick_transfer
"""

remote_client = False # set to True to run the client on AWS
latency_max_load = 10
latency_max_in_flight = 1000

def initialize_client(remote_client=remote_client):
	if remote_client:
		execute(cross_compile)
		run('killall client || true')
		run('rm -r * || true')
		put(committee_config, '.')
		put(accounts, '.')
		put(linux_bin_dir+'client', '.')
		run('chmod +x *')
	else:
		local('(cd ../fastpay && cargo build --release)')

@roles('client')
def get_balance(remote_client=remote_client):
	execute(initialize_client, remote_client)

	f = open(initial_accounts, 'r')
	addresses = f.read().splitlines()
	f.close()
	assert len(addresses) > 0
	for addr in addresses:
		command = './client --committee %s --accounts %s query_balance %s' \
			% (committee_config, accounts, addr)
		if remote_client:
			run(command)
		else:
			execute(local_run, command)

@roles('client')
def transfer(remote_client=remote_client):
	execute(initialize_client, remote_client)

	f = open(initial_accounts, 'r')
	addresses = f.read().splitlines()
	f.close()
	assert len(addresses) > 0

	for i, sender in enumerate(addresses):
		command = './client --committee %s --accounts %s transfer --from %s --to %s 1' \
			% (committee_config, accounts, sender, sender)
		if remote_client:
			run(command)
		else:
			execute(local_run, command)

@roles('client')
def mass_transfer(remote_client=remote_client):
	execute(initialize_client, remote_client)

	command = './client --committee %s --accounts %s benchmark --max_orders %d --max_in_flight %d' \
		% (committee_config, accounts, latency_max_load, latency_max_in_flight)

	if remote_client:
		run(command)
	else:
		execute(local_run, command)

@roles('client')
def quick_transfer(remote_client=remote_client):
	# this function is used to test latency with crashed nodes.
	execute(initialize_client, remote_client)

	f = open(initial_accounts, 'r')
	addresses = f.read().splitlines()
	f.close()
	assert len(addresses) > 0

	#recipient = 'raG+Bvmb9Wp4IdR3D8KmyRwZ8Wadf63A2liTiGkopHY='
	for i, sender in enumerate(addresses):
		command = './client --committee %s --accounts %s quick_transfer --from %s --to %s 1' \
			% (committee_config, accounts, sender, sender)
		if remote_client:
			run(command)
		else:
			execute(local_run, command)

"""
Run throughput measurement (run on a dedicated machine).
COMMANDS:	fab set_hosts update 	updates the binaries
			fab set_hosts tps 		run throughput benchmark
"""

@roles('throughput')
def background_run(command):
	command = 'nohup %s &> /dev/null &' % command
	run(command, pty=False)

@roles('throughput')
def update():
	execute(cross_compile)
	run('killall client || true')
	run('killall server || true')
	run('rm -r * || true')
	put(linux_bin_dir+'client', '.')
	put(linux_bin_dir+'server', '.')
	put('bench.sh', '.')
	run('chmod +x *')
	sudo('sysctl -w net.core.rmem_max=96214400') # increase network buffer
	sudo('sysctl -w net.core.rmem_default=96214400') # increase network buffer

@roles('throughput')
@parallel
def tps(tps_shards=65, tps_accounts=1000000, tps_max_in_flight=1000, tps_committee=4, tps_protocol='UDP', log_file=None):
	shards = int(tps_shards)
	accounts = int(tps_accounts)
	max_in_flight = int(tps_max_in_flight)
	committee = int(tps_committee)
	protocol = tps_protocol
	log = '2>> %s' % log_file if log_file is not None else ''
	run('./bench.sh %d %d %d %d %s aws %s' % (shards, accounts, max_in_flight, committee, protocol, log))

"""
Run throughput measurements for different parameters (run on a dedicated machine), and dump output to file
COMMANDS:	fab set_hosts tps_measurements		run tps measurements for different parameters
			fab set_hosts download_logs			download logs from all benchmark servers
"""

tps_shards = range(15, 86, 10) # the machine has 48 physical CPUs
tps_accounts = 1000000
tps_in_flights = 1000
tps_committee = 4
tps_protocol = 'UDP'

base_tps_log_file = 'tps_logs.txt'

@roles('throughput')
def clean_logs():
	run('rm -r *txt* || true')

def tps_measurements_11_12():
	# settings
	in_flights = [100, 1000, 10000, 50000]

	execute(clean_logs)
	for in_flight in in_flights:
		print('Running measurements with %d in-flights orders.' % in_flight)
		for shards in tps_shards:
			print('Running measurements with %d shards.' % shards)
			tps_log_file = '%d-%d-%d-%d-%s-%s' % (shards, tps_accounts, in_flight, tps_committee, tps_protocol, base_tps_log_file)
			execute(tps, shards, tps_accounts, in_flight, tps_committee, tps_protocol, tps_log_file)
			time.sleep(2)

def tps_measurements_13_14():
	# settings
	accounts = [150000, 500000, 1000000, 1500000]

	execute(clean_logs)
	for account in accounts:
		print('Running measurements with %d transaction load.' % account)
		for shards in tps_shards:
			print('Running measurements with %d shards.' % shards)
			tps_log_file = '%d-%d-%d-%d-%s-%s' % (shards, account, tps_in_flights, tps_committee, tps_protocol, base_tps_log_file)
			execute(tps, shards, account, tps_in_flights, tps_committee, tps_protocol, tps_log_file)
			time.sleep(2)

def tps_measurements_15():
	# settings
	shards = [75]	#shards = [75, 45]
	committees = [3*f+1 for f in range(10, 33)] #committees = [3*f+1 for f in range(1, 10)]

	execute(clean_logs)
	for shard in shards:
		print('Running measurements with %d shards.' % shard)
		for committee in committees:
			print('Running measurements with %d authorities.' % committee)
			tps_log_file = '%d-%d-%d-%d-%s-%s' % (shard, tps_accounts, tps_in_flights, committee, tps_protocol, base_tps_log_file)
			execute(tps, shard, tps_accounts, tps_in_flights, committee, tps_protocol, tps_log_file)
			time.sleep(2)

def download_logs():
	for host in env.roledefs['throughput']:
		with settings(host_string=host):
			run('for f in *.txt; do mv -i "$f" "$f.%s" || true; done' % host)
			get('*.txt*', './raw_logs')
