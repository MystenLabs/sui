#  Narwhal Cluster Startup

## Introduction

This directory contains the configuration information needed to
quickly setup and spin-up a small Narwhal cluster via [Docker Compose](https://docs.docker.com/compose/).

In this directory, you will find:
* The `Dockerfile` definition for a Narwhal node
* A `docker-compose.yml` file to allow you to quickly create a Narwhal cluster

## Quick start

First, you must install:

* [Docker](https://docs.docker.com/get-docker/)
* [Docker-compose](https://docs.docker.com/compose/install/)

Afterward, you will start the Narwhal cluster. 

First, **make sure that you are on the `Docker folder`** . In the rest of the
document, we'll assume that we are under this folder:
```
$ cd Docker     # Change to Docker directory
$ pwd           # Print the current directory 
narwhal/Docker
```

Then bring up the cluster via the following command:
```
$ docker-compose -f docker-compose.yml up
```

The first time this runs, `docker-compose` will build the Narwhal docker image. (This can take a few minutes
since the narwhal node binary needs to be built from the source code.) And then it will spin up 
a cluster for *four nodes* by doing the necessary setup for `primary` and `worker` nodes. Each
`primary` node will be connected to *one worker* node.

The logs from the `primary` and `worker` nodes are available via
```
docker-compose logs primary_<num>

docker-compose logs worker_<num>
```

	By default, the production (release) version of the Narwhal node will be compiled when the Docker image is being built.
To build the Docker image with the development version of Narwhal, which will lead to smaller compile times and
smaller binary (and image) size, you run the `docker-compose` command as:
```
$ docker-compose build --build-arg BUILD_MODE=debug
```

And then run:

```
$ docker-compose up
```

> **Warning**: By default, each validator's directory will be cleaned up between `docker-compose` runs when each node
> bootstraps. To preserve those logs between runs, employ the environment variable `CLEANUP_DISABLED` as described in
> [Docker Compose configuration](#docker-compose-configuration).

## Build Docker image without docker-compose

To build the Narwhal node image without `docker-compose`, run:
```
$ docker build -f Dockerfile ../ --tag narwhal-node:latest
```

Since the [Dockerfile](Dockerfile) is located under a different folder from the source code,
it is important to define the context (ex. `../`) and allow the Dockerfile to properly **copy**
the source code to be compiled in later steps.

## Access primary node public gRPC endpoints

The nodes by default are running with the `Tusk` algorithm disabled, which basically allows
you to treat Narwhal as a pure mempool. When that happens, the gRPC server is bootstrapped
for the primary nodes, and that allows interaction with the node (ex. the consensus layer).

The gRPC server for a primary node is running on port `8000`. However, by default, a container's port
is not accessible to hit by the host (local) machine unless it's exported a mapping between a host's
machine port and the corresponding container's port (ex. for someone to use a gRPC client on their
computer to hit a primary's node container gRPC server). The [docker-compose.yml](docker-compose.yml) file 
exports the gRPC port for each primary node so they can be accessible from the host machine.

For the default setup of *four primary* nodes, the gRPC servers are listening to the following
local (machine) ports:
* `primary_0`: 8000
* `primary_1`: 8001
* `primary_2`: 8002
* `primary_3`: 8003

For example, to send a gRPC request to the `primary_1` node, use the URL: `127.0.0.1:8001`

## Access worker node public gRPC endpoints

Just as you access the [public gRPC endpoints on a primary node](#access-primary-node-public-grpc-endpoints), you may
similarly **feed transactions** to the Narwhal cluster via the `worker` nodes with the gRPC server
bootstrapped on the worker nodes bind to the local machine port. To send transactions, the following local
ports can be used:
* `worker_0`: 7001
* `worker_1`: 7002
* `worker_2`: 7003
* `worker_3`: 7004

For example, to send a transaction to the `worker_2` node via gRPC, the url `127.0.0.1:7003` should be used.

## Folder structure

Here is the Docker folder structure:

```
├── Dockerfile
├── README.md
├── validators
│   ├── validator-0
│   │   └── key.json
│   ├── validator-1
│   │   └── key.json
│   ├── validator-2
│   │   └── key.json
│   ├── validator-3
│   │   └── key.json
│   ├── committee.json
│   └── parameters.json
├── docker-compose.yml
└── entry.sh
```

Under the `validators` folder find the independent configuration
folder for each validator node. (Remember, each `validator` is 
constituted from one `primary` node and several `worker` nodes.)

The `key.json` file contains the private `key` for the corresponding node that
is associated with this node only.

The [parameters.json](validators/parameters.json) file is shared across all the nodes and contains
the core parameters for a node.

The [committee.json](validators/committee.json) file is shared across all the nodes and contains
the information about the validators (primary & worker nodes), like the public keys, addresses,
ports available, etc.

Note the current `docker-compose` setup is mounting the [Docker/validators](validators)
folder to the service containers in order to share the folders and files in it. That allows us to experiment/change
configuration without having to rebuild the Docker image.

## Docker Compose configuration

The following environment variables are available to be used for each service in the
[docker-compose.yml](docker-compose.yml) file configuration:
* `NODE_TYPE` with values `primary|worker`. Defines the node type to bootstrap.
* `AUTHORITY_ID` with decimal numbers, for current setup available values `0..3`. Defines the
ID of the validator that the node/service corresponds to. This defines which
configuration to use under the `validators` folder.
* `LOG_LEVEL` is the level of logging for the node defined as number of `v` parameters (ex `-vvv`). The following
levels are defined according to the number of "v"s provided: `0 | 1 => "error", 2 => "warn", 3 => "info", 
4 => "debug", 5 => "trace"`.
* `CONSENSUS_DISABLED`. This value disables consensus (`Tusk`) for a primary node and enables the
`gRPC` server. The corresponding argument is: `--consensus-disabled`
* `WORKER_ID` is the ID, as integer, for service when it runs as a worker.
* `CLEANUP_DISABLED`, when provided with value `true`, will disable the clean up of the validator folder
from the database and log data. This is useful to preserve the state between multiple Docker Compose runs.

## How to run more than the default 4 nodes with docker compose.
### Prerequisites:
 - python3
 - You must build the narwhal `node` binary at top level:

   ```cargo build --release --all-features```
   
   That binary is necessary for generating the keys for the validators and the committee.json seed file.
   
### Running the `gen.validators.sh #` script to generate a larger cluster.


```
./gen.validators.sh 6

# That will create a docker-compose.yaml file in ./validators-6/docker-compose.yaml

cd validators-6

docker compose up -d
docker compose logs -f

```

Note within the validators-#/ directory that the `committee.json` file is generated.
The `parameters.json` is (so far) a static template and just dropped into that dir.

Also note that the primaries are created with only 1 worker node currently.
When multiple workers are needed we'll add that feature.


## Grafana, Prometheus and Loki.

The grafana instance is exposed at http://localhost:3000/

Default user/pass is admin/admin.

You can 'skip' changing that since it's always regenerated.

Grafana is the frontend dashboard and metrics explorer, as well as a means
for setting up alerts.
	- https://grafana.com/oss/grafana/
	- https://grafana.com/grafana/dashboards/ published dashboards, good place to start building.

Prometheus is the defacto standard for pulling metrics from targets and
storing for use via Grafana and other services (alertmanager, scripts).
	- https://prometheus.io/docs/introduction/overview/

Loki is a log collector and processor.  It is exposed as a datasource
in Grafana and makes the logs easily searchable.
	- https://grafana.com/oss/loki/

Currently there are no Loki dashboards defined, however you can
browse the logs via the "Explorer", selecting the Loki datasource.


## Troubleshooting

#### 1. Compile errors when building Docker image
If you encounter errors while the Docker image is being built, for example errors like:
```
error: could not compile `tonic`
#9 373.3 
#9 373.3 Caused by:
#9 373.4   process didn't exit successfully: `rustc --crate-name tonic --edition=2018
....
#9 398.4 The following warnings were emitted during compilation:
#9 398.4 
#9 398.4 warning: c++: fatal error: Killed signal terminated program cc1plus
#9 398.4 warning: compilation terminated.
```

it is possible that the Docker engine is running out of memory, and there is no capacity to properly
compile the code. In this case please, increase the available RAM to at least 2GB and retry.

### 2. Mounts denied or cannot start service errors

If you try to spin up the nodes via `docker-compose` and you come across errors such as `mounts denied`
or `cannot start service`, make sure that you allow Docker to share your host's [Docker/validators](validators) folder 
with the containers. If you are using Docker Desktop, you can find more information on how to do
that here: [mac](https://docs.docker.com/desktop/mac/#file-sharing), [linux](https://docs.docker.com/desktop/linux/#file-sharing),
[windows](https://docs.docker.com/desktop/windows/#file-sharing) .

Also, check that you are not using the deprecated `devicemapper storage driver`, which might also
cause you issues. See how to [migrate to an overlayfs driver](https://docs.docker.com/storage/storagedriver/overlayfs-driver/) . 
More information about the deprecation can be found [here](https://docs.docker.com/engine/deprecated/#device-mapper-storage-driver) 
