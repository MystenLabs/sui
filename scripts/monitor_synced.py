#!/usr/bin/env python3
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

import json
import os
import sys
import subprocess
import getopt
from enum import Enum
import time
from datetime import datetime


NUM_RETRIES = 5
CHECKPOINT_STUCK_THRESHOLD_SEC = 10
EPOCH_STUCK_THRESHOLD_SEC = 10 * 60


class Metric(Enum):
    CHECKPOINT = 'last_executed_checkpoint'
    EPOCH = 'current_epoch'


def get_current_network_epoch(env='testnet'):
    for i in range(NUM_RETRIES):
        cmd = ['curl', '--location', '--request', 'POST', f'https://explorer-rpc.{env}.sui.io/',
               '--header', 'Content-Type: application/json', '--data-raw',
               '{"jsonrpc":"2.0", "method":"suix_getEpochs", "params":[null, 1, true], "id":1}']
        try:
            result = subprocess.check_output(cmd, stderr=subprocess.PIPE)
        except subprocess.CalledProcessError as e:
            print(f'curl command failed with error {e.returncode}: {e.output}')
            time.sleep(3)
            continue

        try:
            result = json.loads(result)
            return int(result['result']['data'][0]['epoch'])
        except (KeyError, IndexError, json.JSONDecodeError):
            print(f'suix_getEpochs rpc request failed: {result}')
            time.sleep(3)
            continue
    print(f"Failed to get current network epoch after {NUM_RETRIES} tries")
    exit(1)


def get_local_metric(metric: Metric):
    for i in range(NUM_RETRIES):
        # try:
        curl = subprocess.Popen(
            ['curl', '-s', 'http://localhost:9184/metrics'], stdout=subprocess.PIPE)
        grep_1 = subprocess.Popen(
            ['grep', metric.value], stdin=curl.stdout, stdout=subprocess.PIPE)
        try:
            result = subprocess.check_output(
                ['grep', '^[^#;]'], stdin=grep_1.stdout, stderr=subprocess.PIPE)
        except subprocess.CalledProcessError as e:
            print(f'curl command failed with error {e.returncode}: {e.output}')
            time.sleep(3)
            continue

        # convert json result to dictionary
        try:
            return int(result.split()[1])
        except (KeyError, IndexError, json.JSONDecodeError):
            print(
                f'Failed to get local metric {metric.value}: {result.stdout}')
            time.sleep(3)
            continue
    print(
        f"Failed to get local metric {metric.value} after {NUM_RETRIES} tries")
    exit(1)


def main(argv):
    if len(argv) > 2:
        print(
            "Usage: monitor_synced.py [--end-epoch=END_EPOCH] [--env=ENVIRONMENT]")
        exit(1)

    opts, args = getopt.getopt(argv, '', ["env=", "end-epoch="])

    env = 'testnet'
    end_epoch = None
    for opt, arg in opts:
        if opt == '--env':
            env = arg
        elif opt == '--end-epoch':
            end_epoch = int(arg)

    if end_epoch is None:
        end_epoch = get_current_network_epoch(env)
    print(f'Will attempt to sync to epoch {end_epoch}')

    current_epoch = get_local_metric(Metric.EPOCH)
    print(f'Current local epoch: {current_epoch}')
    start_epoch = current_epoch

    current_checkpoint = get_local_metric(Metric.CHECKPOINT)
    print(f'Locally highest executed checkpoint: {current_checkpoint}')
    start_checkpoint = current_checkpoint

    # get local time so that we can measure the time since the last epoch change
    current_time = datetime.now()
    start_time = current_time
    while current_epoch < end_epoch:
        # check that we are making progress
        time.sleep(10)
        new_checkpoint = get_local_metric(Metric.CHECKPOINT)

        if new_checkpoint == current_checkpoint:
            print(
                f'Checkpoint is stuck at {current_checkpoint} for over {CHECKPOINT_STUCK_THRESHOLD_SEC} seconds')
            exit(1)
        current_checkpoint = new_checkpoint

        new_epoch = get_local_metric(Metric.EPOCH)
        if new_epoch > current_epoch:
            current_epoch = new_epoch
            print(f'New local epoch: {current_epoch}')
            current_time = datetime.now()
        else:
            # check if we have been stuck for more than 5 minutes
            if (datetime.now() - current_time).total_seconds() > EPOCH_STUCK_THRESHOLD_SEC:
                print(
                    f'Epoch is stuck at {current_epoch} for over {EPOCH_STUCK_THRESHOLD_SEC} seconds')
                exit(1)
        print(f'New highest executed checkpoint: {current_checkpoint}')

    elapsed_minutes = (datetime.now() - start_time).total_seconds() / 60
    print('-------------------------------')
    print(f"Successfully synced to epoch {end_epoch} from epoch {start_epoch} ({current_checkpoint - start_checkpoint} checkpoints) in {elapsed_minutes:.2f} minutes")
    exit(0)


if __name__ == "__main__":
    main(sys.argv[1:])
