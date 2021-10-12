# Copyright (c) Facebook, Inc. and its affiliates.
# SPDX-License-Identifier: Apache-2.0

import numpy as np

# benchmark of serialize_tests
# run: cargo test --release time -- --nocapture

write_order = [27, 31, 27, 27, 27]
write_vote = [27, 31, 26, 27, 25]
write_cert = [4, 4, 4, 5, 4]

read_and_check_order = [58, 61, 58, 59, 58]
read_and_check_vote = [60, 62, 60, 60, 58]
read_and_check_cert = [235, 249, 219, 245, 236]

print('Write Order: %d (average), %d (std)' % (np.mean(write_order), np.std(write_order)))
print('Write Vote: %d (average), %d (std)' % (np.mean(write_vote), np.std(write_vote)))
print('Write Cert: %d (average), %d (std)' % (np.mean(write_cert), np.std(write_cert)))

print('Read & Check Order: %d (average), %d (std)' %
	(np.mean(read_and_check_order), np.std(read_and_check_order)))

print('Read & Check Vote: %d (average), %d (std)' %
	(np.mean(read_and_check_vote), np.std(read_and_check_vote)))

print('Read & Check Cert: %d (average), %d (std)' %
	(np.mean(read_and_check_cert), np.std(read_and_check_cert)))
