# Copyright (c) Facebook, Inc. and its affiliates.
# SPDX-License-Identifier: Apache-2.0

import numpy as np
import matplotlib.pyplot as plt
import re
import sys
import os.path
import argparse
import os, fnmatch
from collections import Counter, OrderedDict
import operator

SHARDS = 0
LOAD = 1
IN_FLIGHTS = 2
COMMITTEE = 3

"""
Parsed raw logs and prints to disc the following dictionary, where '<Z_VALUE>' is the parameter of the grpah,
<X_VALUE> is the value of the x-axis, and <Y_VALUE> is the throughput:

{
	'transfer': {
		'<Z_VALUE>': [
			[(<X_VALUE>, <Y_VALUE>)]
		],
	},
	'confirmation': {
		'<Z_VALUE>': [
			[(<X_VALUE>, <Y_VALUE>)]
		],
	}
}
"""
def parse(log_file, parsed_log_file, x_axis=SHARDS, z_axis=IN_FLIGHTS):
	fname = os.path.abspath(log_file)
	data = open(fname).read()

	parameters = re.findall(r'\d+', log_file)
	x_value = parameters[x_axis]
	z_value = parameters[z_axis]
	#_accounts = parameters[1] # not used
	#in_flights = parameters[2]
	#_committee = parameters[3] # not used

	orders_types = ['transfer', 'confirmation']
	orders = {}
	for orders_type in orders_types:
		orders[orders_type] = {}
		tps = ''.join(re.findall(r'Estimated server throughput: [0-9]* %s orders per sec' % orders_type, data))
		tps = re.findall(r'\d+',tps)
		assert len(tps) == 1
		orders[orders_type][z_value] = [(x_value, tps[0])]

	with open(parsed_log_file, 'w') as f:
		f.write(str(orders))

"""
Aggregate parsed logs and prints to disc the following dictionary, where '<Z_VALUE>' is the parameter of the grpah,
<X_VALUE> is the value of the x-axis, and <Y_VALUE> is the throughput

{
	'transfer': {
		'<Z_VALUE>': [
				[(<X_VALUE>, <Y_VALUE>), (<X_VALUE>, <Y_VALUE>), ...],
				...
			],
		'<Z_VALUE>': [
				[(<X_VALUE>, <Y_VALUE>), (<X_VALUE>, <Y_VALUE>), ...],
				...
			],
		...
	},
	'confirmation': {
		'<Z_VALUE>': [
				[(<X_VALUE>, <Y_VALUE>), (<X_VALUE>, <Y_VALUE>), ...],
				...
			],
		'<Z_VALUE>': [
				[(<X_VALUE>, <Y_VALUE>), (<X_VALUE>, <Y_VALUE>), ...],
				...
			],
		...
	}
}
"""
def aggregate(parsed_log_files, aggregated_parsed_log_file):
	assert len(parsed_log_files) > 1

	with open(parsed_log_files[0], 'r') as f:
		aggregate_orders = eval(f.read())

	for parsed_log_file in parsed_log_files[1:]:
		with open(parsed_log_file, 'r') as f:
			data = eval(f.read())
			for (orders_type, orders) in data.items():
				assert len(orders.items()) == 1
				(z_value, items) = list(orders.items())[0]
				assert len(items) == 1
				if z_value in aggregate_orders[orders_type]:
					aggregate_orders[orders_type][z_value] += items
				else:
					aggregate_orders[orders_type][z_value] = items

	for (orders_type, orders) in aggregate_orders.items():
		for (z_value, items) in orders.items():
			items.sort(key=lambda tup: int(tup[0]))
			counter = Counter(item[0] for item in items)
			shards = len(counter.items())
			runs = list(counter.values())[0]
			assert runs * shards == len(items)
			arr = np.array(items)
			items = arr.reshape((shards,runs,2)).tolist()
			aggregate_orders[orders_type][z_value] = items
	print(aggregate_orders)

	with open(aggregated_parsed_log_file, 'w') as f:
		f.write(str(aggregate_orders))


"""
Load parsed logs (as produced by 'parse'), and saves the following figures as PDF:
	- the throughput of transfer orders VS the number of processes, for multiple max in-flight values
	- the throughput of confirmation orders VS the number of processes, for multiple max in-flight values
"""
def plot(parsed_log_file, x_label='Number of shards', z_label='tx in-flight', legend_position='lower right', style='plot'):
	with open(parsed_log_file, 'r') as f:
		orders = eval(f.read())

	for (orders_type, order) in orders.items():
		fig = plt.figure()
		width = 2
		i = -3
		for (z_value, items) in sorted(order.items(), reverse=True):
			x_values = []
			y_values = []
			y_err = []
			for item in items:
				x, y = list(zip(*item))
				x = int(x[0])
				y = np.array(y).astype(np.int)
				x_values.append(x)
				y_values.append(np.mean(y))
				y_err.append(np.std(y))
			if style == 'bar':
				plt.bar(np.array(x_values) + i, y_values, width, yerr=y_err,
	            	label='%s %s' % (z_value, z_label))
				i = i + width
				plt.xticks(x_values, x_values)
			else:
				#plt.plot(x_values, y_values)
				plt.ylim(0, 180000)
				plt.errorbar(x_values, y_values, yerr=y_err, uplims=True, lolims=True,
	            	label='%s %s' % (z_value, z_label), marker='.', alpha=1, dashes=None)

		plt.legend(loc=legend_position)
		plt.xlabel(x_label)
		plt.ylabel('Observed throughput (tx / sec)')
		plt.savefig('%s.pdf' % orders_type)
		print('created figure "%s.pdf".' % orders_type)

"""
Utility to find files
"""
def find(pattern, path):
    result = []
    for root, dirs, files in os.walk(path):
        for name in files:
            if fnmatch.fnmatch(name, pattern):
                result.append(os.path.join(root, name))
    return result

if __name__== '__main__':
	aggregated_log = 'aggregated_tps_log.txt'
	commands = ['parse', 'aggregate', 'plot', 'all']
	command = 'plot'

	'''
	parser = argparse.ArgumentParser()
	parser.add_argument('-c', action='store', dest='command', help = 'Command to execute (parse, aggregate, plot).')
	args = vars(parser.parse_args())
	command = args['command']
	print(args)
	'''

	execute_all = command == commands[3]
	if command == commands[0] or execute_all:
		raw_logs = find('*.txt.*.*.*.*', '.')
		parsed_logs = ['%s_parsed' % raw_log for raw_log in raw_logs]
		[parse(raw_log, parsed_log, x_axis=SHARDS, z_axis=IN_FLIGHTS) for (raw_log, parsed_log) in zip(raw_logs, parsed_logs)]

	if command == commands[1] or execute_all:
		parsed_logs = find('*_parsed', '.')
		aggregate(parsed_logs, aggregated_log)

	if command == commands[2] or execute_all:
		plot(aggregated_log, x_label='Committee size', z_label='tx shards')
