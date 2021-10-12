# Copyright (c) Facebook, Inc. and its affiliates.
# SPDX-License-Identifier: Apache-2.0

"""
Make plots on AWS as installing Matplotlib on macOS is a pain.
Then downloads parsed logs.
"""

from fabric.api import run, roles, execute, task, put, env, local, sudo, settings, get
from fabric.contrib import files
import boto3
from botocore.exceptions import ClientError
import os

ec2 = boto3.client('ec2')

region = os.environ.get("AWS_EC2_REGION")
env.user = 'ubuntu'
env.key_filename = '~/.ssh/aws-fb.pem' 	# SET PATH TO KEY FILE HERE

"""
Initialise hosts, this command is run in conjunction with other commands.
COMMANDS:	fab -f aws_plot.py set_hosts <COMMAND>
"""

instances_id = [] # automatically filled by 'set_hosts'
env.roledefs = {'dev': [], } # automatically filled by 'set_hosts'

def set_hosts(value ='running', instance_type='dev', region=region):
	ec2resource = boto3.resource('ec2')
	instances = ec2resource.instances.filter(Filters=[{'Name': 'instance-state-name', 'Values': [value]}])
	for instance in instances:
		for tag in instance.tags or []:
			if 'Name'in tag['Key']:
				name = tag['Value']
		if instance_type in name:
			instances_id.append(instance.id)
			env.roledefs['dev'].append(instance.public_ip_address)

"""
Print commands to ssh into hosts (debug).
COMMANDS:	fab -f aws_plot.py info
"""

def info():
	set_hosts()

	print('\ndev:')
	for dev in env.roledefs['dev']:
		print('\t ssh -i '+env.key_filename+' '+env.user+'@'+dev)

"""
Parse logs and create plots; then donwloads plots and parsed logs.
COMMANDS:	fab -f aws_plot.py set_hosts clean
			fab -f aws_plot.py set_hosts throughput
			fab -f aws_plot.py set_hosts latency
"""

tps_script = 'throughput.py'
tps_raw_logs = 'raw_logs/*'
latency_script = 'latency.py'
latency_raw_logs = 'latency_raw_logs/*'
subdir = 'fastpay/'

@roles('dev')
def clean():
	run('rm -r '+subdir+'* || true')
	#put(tps_raw_logs, subdir+'.')
	#put(latency_raw_logs, subdir+'.')

@roles('dev')
def throughput():
	put(tps_script, subdir+'.')
	#put('aggregated_parsed_logs/*tps*', subdir+'.')
	run('(cd '+subdir+' && python3 throughput.py)')
	if files.exists(subdir+'*aggregated*'):
		get(subdir+'*aggregated*', '.')
	get(subdir+'*.pdf', '.')
	local('open *.pdf')

@roles('dev')
def latency():
	put(latency_script, subdir+'.')
	#put(latency_raw_logs, subdir+'.')
	run('(cd '+subdir+' && python3 %s plot)' % latency_script)
	if files.exists(subdir+'*aggregated*'):
		get(subdir+'*aggregated*', '.')
	get(subdir+'*.pdf', '.')
	local('open latency*.pdf')
