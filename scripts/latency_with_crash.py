# Copyright (c) Facebook, Inc. and its affiliates.
# SPDX-License-Identifier: Apache-2.0

import numpy as np
import re
import sys
import os.path
import os, fnmatch


def parse(row_log_file, parsed_log_file):
	fname = os.path.abspath(row_log_file)
	data = open(fname).read()

	latency = ''.join(re.findall(r'Received certificate after [0-9]* us', data))
	latency = re.findall(r'\d+',latency)
	latency = [int(v)/1000 for v in latency]

	print(row_log_file)
	print('%d ms (average), %d ms (std)' % (np.mean(latency), np.std(latency)))
	print('\n')

def find(pattern, path):
    result = []
    for root, dirs, files in os.walk(path):
        for name in files:
            if fnmatch.fnmatch(name, pattern):
                result.append(os.path.join(root, name))
    return result

'''
Experiment stes:
	1. Run a testnet with 10 authorities:
		fab set_hosts reset deploy

	2. Submit transactions:
		fab set_hosts quick_transfer

	3. Kill one node, and go at step 2; then repeat.
'''
if __name__== '__main__':
	raw_logs = find('raw_log_latency_with_crash-*.txt', '.')
	parsed_logs = ['parsed_%s' % os.path.basename(raw_log) for raw_log in raw_logs]
	[parse(raw_log, parsed_log) for (raw_log, parsed_log) in zip(raw_logs, parsed_logs)]
