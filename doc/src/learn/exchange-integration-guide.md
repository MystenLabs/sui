---
title: Sui Exchange Integration Guide
---

This topic describes how to integrate SUI, the token native to the Sui network, into a cryptocurrency exchange. The specific requirements and processes to implement an integration vary between exchanges. Rather than provide a step-by-step guide, this topic provides information about the primary tasks necessary to complete an integration. After the guidance about how to configure an integration, you can also find information and code samples related to staking and delegation on the Sui network.

## Requirements to configure a SUI integration

The requirements to configure a SUI integration include:
 * A Sui Full node. You can operate your own Sui Full node or use a Full node from a node operator.
 * Suggested hardware requirements to run a Sui Full node:
    * CPU: 10 core
    * RAM: 32 GB
    * Storage: 1 TB SSD

We recommend running Sui Full nodes on Linux. Sui supports the Ubuntu and Debian distributions.

## Configure a Sui Full node

You can set up and configure a Sui Full node using Docker or directly from source code in the Sui GitHub repository.

### Install a Sui Full node using Docker

Run the command in this section using the same branch of the repository for each. Replace `branch-name` with the branch you use. For example, use `devnet` to use the Sui Devnet network, or use `testnet` to use the Sui Testnet network.

 1. Install [Docker](https://docs.docker.com/get-docker/) and [Docker Compose](https://docs.docker.com/compose/install/). Docker Desktop version installs Docker Compose.
 1. Install dependencies for Linux:
    ```bash
    apt update \
    && apt install -y --no-install-recommends \ 
    tzdata \ 
    ca-certificates \ 
    build-essential \ 
    pkg-config \ 
    cmake
    ```
 1. Download the docker-compose.yaml file:
    ```bash
    wget https://github.com/MystenLabs/sui/blob/branch-name/docker/fullnode/docker-compose.yaml
    ```
 1. Download the fullnode-template.yaml file:
    ```bash
    wget https://github.com/MystenLabs/sui/raw/branch-name/crates/sui-config/data/fullnode-template.yaml
    ```
 1. Download the genesis.blob file:
    ```bash
    wget https://github.com/MystenLabs/sui-genesis/raw/main/branch-name/genesis.blob
    ```
 1. Start the Full node. The -d switch starts it in the background (detached mode).
    ```bash
    docker-compose up -d
    ```
