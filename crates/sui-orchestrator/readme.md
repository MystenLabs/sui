# Sui Orchestrator

The crate provides facilities to quickly deploy and benchmark the Sui codebase in a geo-distributed environment. It is absolutely not meant to run Sui in production and is no indicator of proper production engineering best practices. Its purpose is to facilitate research projects wishing to benchmarks (variants of) Sui and analyze its performance.

Below is a step-by-step guide to run geo-distributed benchmarks on either [Vultr](http://vultr.com) or [Amazon Web Services (AWS)](http://aws.amazon.com).

## Step 1. Set up cloud provider credentials

Set up your cloud provider credentials to enable programmatic access to your account from your local machine. These credentials authorize your machine to create, delete, and edit instances on your account programmatically.

### Setup Vultr credentials

Find your ['Vultr token'](https://www.vultr.com/docs/). Then, create a file `~/.vultr` containing only your access token:

```text
YOUR_ACCESS_TOKEN
```

### Set up AWS credentials

Find your ['access key id' and 'secret access key'](https://docs.aws.amazon.com/cli/latest/userguide/cli-configure-quickstart.html#cli-configure-quickstart-creds). Then, create a file `~/.aws/credentials` with the following content:

```text
[default]
aws_access_key_id = YOUR_ACCESS_KEY_ID
aws_secret_access_key = YOUR_SECRET_ACCESS_KEY
```

Do not specify any AWS region in that file as the python scripts will allow you to handle multiple regions programmatically.

## Step 2. Specify the testbed configuration

Create file `settings.json` containing all the configuration parameters of the testbed to deploy. An example file can be found in `./assets/settings.json` and its content looks as follows:

```json
{
  "testbed_id": "alberto-sui",
  "cloud_provider": "aws",
  "token_file": "/Users/alberto/.aws/credentials",
  "ssh_private_key_file": "/Users/alberto/.ssh/aws",
  "regions": [
    "us-east-1",
    "us-west-2",
    "ca-central-1",
    "eu-central-1",
    "ap-northeast-1",
    "eu-west-1",
    "eu-west-2",
    "ap-south-1"
  ],
  "specs": "g5.8xlarge",
  "repository": {
    "url": "http://github.com/mystenlabs/sui",
    "commit": "orchestrator"
  },
  "results_directory": "./results",
  "logs_directory": "./logs"
}
```

Look at the rust struct `Settings` in `./src/settings.rs` for details about each field.

## Step 4. Create a testbed

The `orchestrator` binary provides several facilities to create, start, stop, and destroy instances. The following command boots 2 instances per region. That is, if the setting file specified 8 regions (as in the example above) the command boots a total of 16 instances.

```bash
cargo run --bin orchestrator -- testbed deploy --instances 2
```

The following command displays the current status of the instances of the testbed:

```bash
cargo run --bin orchestrator testbed status
```

All instances listed with a green number are available and ready for use; instances listed with a red number are stopped.

## Step 5. Running benchmarks

Running benchmarks involves installing on the remote machines the version of the Sui codebase specified in the settings as well as running one Sui validator and one load generator per instance. For instance, the following command benchmarks a committee of 10 validators when submitted to a constant load of 200 tx/s for a duration of 3 minutes.

```bash
cargo run --bin orchestrator -- benchmark --committee 10 --loads 200 --duration 180s
```

Since a network of 10 validators runs with 10 loads generators (each validator is collocated with a load generator), each load generator submits a fixed load of 20 tx/s. Performance measurements are collected by regularly scraping the prometheus metrics exposed by the load generators.

## Step 6. Analyzing results

Benchmarks results are automatically saved into the folder specified by the settings file. The following command plots a rudimentary L-graph giving to get a quick idea of the system's performance.

```bash
cargo run --bin orchestrator -- plot
```

More elaborated (and nicer) plots can be generated with the python script located in `./assets/plots.rs`.
