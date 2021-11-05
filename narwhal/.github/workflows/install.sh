#!/bin/bash

# Add clang & others
sudo apt-get update

sudo apt-get install -y --no-install-recommends python3-numpy libatlas-base-dev clang-11

# Make python3 default
sudo update-alternatives --install /usr/local/bin/python python /usr/bin/python3 40
sudo pip3 --no-cache-dir install --upgrade pip
