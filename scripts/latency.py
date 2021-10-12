# Copyright (c) Facebook, Inc. and its affiliates.
# SPDX-License-Identifier: Apache-2.0

import numpy as np
import matplotlib.pyplot as plt
import re
import sys
import os.path
import os, fnmatch

MPK = 'U.S. West Coast'
LDN = 'U.K.'

def parse(row_log_file, parsed_log_file):
	location = MPK
	if 'ldn' in row_log_file:
		location = LDN

	fname = os.path.abspath(row_log_file)
	data = open(fname).read()

	latency = ''.join(re.findall(r'Received [0-9]* responses in [0-9]* ms', data))
	latency = re.findall(r'\d+',latency)
	assert len(latency) % 4 == 0
	latency = [int(v) for v in latency]
	latency = np.array(latency).reshape(int(len(latency)/4), 2, 2)
	transfers, confirmations = list(zip(*latency))
	transfers = [i.tolist() for i in transfers]
	confirmations = [i.tolist() for i in confirmations]
	results = {}
	results['transfer'] = {location: [transfers]}
	results['confirmation'] = {location: [confirmations]}

	with open(parsed_log_file, 'w') as f:
		f.write(str(results))

def aggregate(parsed_log_files, aggregated_log_file):
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

	with open(aggregated_log_file, 'w') as f:
		f.write(str(aggregate_orders))
		print('aggregated %d log files' % len(parsed_log_files))

def byz(y):
	byz_y = []
	for i,v in enumerate(y):
		index = int(i/3)*2
		byz_y.append(y[index])
	return byz_y

def plot(aggregated_log_file, x_label='Committee size', legend_position='upper right'):
	with open(aggregated_log_file, 'r') as f:
		orders = eval(f.read())

	for (orders_type, order) in orders.items():
		fig = plt.figure()
		for (z_value, items) in order.items():
			x_values = []
			y_values = []
			y_err = []
			for item in items:
				x, y = list(zip(*item))
				x = int(x[0])
				y = y[15:len(y)-15]
				x_values.append(x)
				y_values.append(np.mean(y))
				y_err.append(np.std(y))

			data = list(zip(x_values, y_values, y_err))
			data.sort(key=lambda tup: tup[0])
			x_values, y_values, y_err = list(zip(*data))
			plt.ylim(0, 300)
			plt.errorbar(x_values, y_values, yerr=y_err, uplims=True, lolims=True,
            	label='%s' % z_value, marker='.', alpha=1, dashes=None)

		plt.legend(loc=legend_position)
		plt.xlabel(x_label)
		plt.ylabel('Observed latency (ms)')

		plt.savefig('latency-%s.pdf' % orders_type)
		print('created figure "latency-%s.pdf".' % orders_type)

def find(pattern, path):
    result = []
    for root, dirs, files in os.walk(path):
        for name in files:
            if fnmatch.fnmatch(name, pattern):
                result.append(os.path.join(root, name))
    return result

if __name__== '__main__':
	aggregated_log = 'aggregated_latency_log.txt'
	commands = ['parse', 'aggregate', 'plot', 'all']
	command = sys.argv[1]

	execute_all = command == commands[3]
	if command == commands[0] or execute_all:
		raw_logs = find('raw_log_latency_*-*.txt', '.')
		parsed_logs = ['parsed_%s' % os.path.basename(raw_log) for raw_log in raw_logs]
		[parse(raw_log, parsed_log) for (raw_log, parsed_log) in zip(raw_logs, parsed_logs)]

	if command == commands[1] or execute_all:
		parsed_logs = find('parsed_raw_log_latency_*.txt', '.')
		aggregate(parsed_logs, aggregated_log)

	if command == commands[2] or execute_all:
		plot(aggregated_log)
