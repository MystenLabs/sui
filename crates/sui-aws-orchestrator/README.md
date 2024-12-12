# Orchestrator

The Orchestrator crate provides facilities for quickly deploying and benchmarking this codebase in a geo-distributed environment. Please note that it is not intended for production deployments or as an indicator of production engineering best practices. Its purpose is to facilitate research projects by allowing benchmarking of (variants of) the codebase and analyzing performance.

This guide provides a step-by-step explanation of how to run geo-distributed benchmarks on [Amazon Web Services (AWS)](http://aws.amazon.com).

## Step 1. Set up credentials

To enable programmatic access to your cloud provider account from your local machine, you need to set up your cloud provider credentials. These credentials authorize your machine to create, delete, and edit instances programmatically on your account.

### Setting up AWS credentials

1. Find your ['access key id' and 'secret access key'](https://docs.aws.amazon.com/cli/latest/userguide/cli-configure-quickstart.html#cli-configure-quickstart-creds).
2. Create a file `~/.aws/credentials` with the following content:

```text
[default]
aws_access_key_id = YOUR_ACCESS_KEY_ID
aws_secret_access_key = YOUR_SECRET_ACCESS_KEY
```

Do not specify any AWS region in that file, as the scripts need to handle multiple regions programmatically.

### Setting up SSH credentials

Running `ssh-keygen -t ed25519 -C "..."` would generate a new ssh key pair under the specified path,
e.g. private key at `~/.ssh/aws` and public key at `~/.ssh/aws.pub`. If the public key is not
at the corresponding private key path with `.pub` extension, then the public key path must be specified
for `ssh_public_key_file` in `settings.json`.

## Step 2. Specify the testbed configuration

Create a file called `settings.json` that contains all the configuration parameters for the testbed deployment. You can find a template at `./assets/settings-template.json`. Example content:

```json
{
	"testbed_id": "alberto-0",
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
		"ap-south-1",
		"ap-southeast-1",
		"ap-southeast-2"
	],
	"specs": "m5d.8xlarge",
	"repository": {
		"url": "https://github.com/MystenLabs/sui.git",
		"commit": "main"
	},
	"results_directory": "./results",
	"logs_directory": "./logs"
}
```

The documentation of the `Settings` struct in `./src/settings.rs` provides detailed information about each `Settings` field.

If you're working with a private GitHub repository, you can include a [private access token](https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/managing-your-personal-access-tokens) in the repository URL. For example, if your access token is `[your_token]`, the repository URL should be formatted as follows:

```json
"repository": {
  "url": "http://[your_token]@github.com/mystenlabs/sui.git",
  "commit": "main"
}
```

## Step 3. Create a testbed

The `sui-aws-orchestrator` binary provides various functionalities for creating, starting, stopping, and destroying instances. You can use the following command to boot 2 instances per region (if the settings file specifies 10 regions, as shown in the example above, a total of 20 instances will be created):

```bash
cargo run --bin sui-aws-orchestrator -- testbed deploy --instances 2
```

To check the current status of the testbed instances, use the following command:

```bash
cargo run --bin sui-aws-orchestrator testbed status
```

Instances listed with a green number are available and ready for use, while instances listed with a red number are stopped.

Also keep in mind that there is nothing stopping you from running the `deploy` command multiple times if you find your self
needing more instances down the line.

## Step 4. Choose protocol

There is support to benchmark either Sui or Narwhal only. To choose which protocol to benchmark, you can set the `Protocol` & `BenchmarkType` field [here](https://github.com/MystenLabs/sui/blob/main/crates/sui-aws-orchestrator/src/main.rs#L33-L34)

```
// Sui
use protocol::sui::{SuiBenchmarkType, SuiProtocol};
type Protocol = SuiProtocol;
type BenchmarkType = SuiBenchmarkType;
// Narwhal
use protocol::narwhal::{NarwhalBenchmarkType, NarwhalProtocol};
type Protocol = NarwhalProtocol;
type BenchmarkType = NarwhalBenchmarkType;
```

## Step 5. Running benchmarks

Running benchmarks involves installing the specified version of the codebase on the remote machines and running one validator and one load generator per instance. For example, the following command benchmarks a committee of 10 validators under a constant load of 200 tx/s for 3 minutes:

```bash
cargo run --bin sui-aws-orchestrator -- benchmark --committee 10 fixed-load --loads 200 --duration 180
```

In a network of 10 validators, each with a corresponding load generator, each load generator submits a fixed load of 20 tx/s. Performance measurements are collected by regularly scraping the Prometheus metrics exposed by the load generators. The `sui-aws-orchestrator` binary provides additional commands to run a specific number of load generators on separate machines.

## Step 6. Monitoring

The orchestrator provides facilities to monitor metrics on clients and nodes. The orchestrator deploys a [Prometheus](https://prometheus.io) instance and a [Grafana](https://grafana.com) instance on a dedicated remote machine. Grafana is then available on the address printed on stdout (e.g., `http://3.83.97.12:3000`) with the default username and password both set to `admin`. You can either create a [new dashboard](https://grafana.com/docs/grafana/latest/getting-started/build-first-dashboard/) or [import](https://grafana.com/docs/grafana/latest/dashboards/manage-dashboards/#import-a-dashboard) the example dashboards located in the `./assets` folder.

## Destroy a testbed

After you have found yourself that you don't need the deployed testbed anymore you can simply run

```
cargo run --bin sui-aws-orchestrator -- testbed destroy
```

that will terminate all the deployed EC2 instances. Keep in mind that AWS is not immediately deleting the terminated instances - this could take a few hours - so in case you want to immediately deploy a new testbed it would be advised
to use a different `testbed_id` in the `settings.json` to avoid any later conflicts (see the FAQ section for more information).

## FAQ

### I am getting an error "Failed to read settings file '"crates/sui-aws-orchestrator/assets/settings.json"': No such file or directory"

To run the tool a `settings.json` file with the deployment configuration should be under the directory `crates/sui-aws-orchestrator/assets`. Also, please make sure
that you run the orchestrator from the top level repo folder, ex `/sui $ cargo run --bin sui-aws-orchestrator`

### I am getting an error "IncorrectInstanceState" with message "The instance 'i-xxxxxxx' is not in a state from which it can be started."" when I try to run a benchmark

When a testbed is deployed the EC2 instances are tagged with the `testbed_id` as dictated in the `settings.json` file. When trying to run a benchmark the tool will try to list
all the EC2 instances on the dictated by the configuration regions. To successfully run the benchmark all the listed instances should be in status
`Running`. If there is any instance in different state , ex `Terminated` , then the above error will arise. Please pay attention that if you `destroy` a deployment
and then immediately `deploy` a new one under the same `testbed_id`, then it is possible to have a mix of instances with status `Running` and `Terminated`, as AWS does not immediately
delete the `Terminated` instances. That can eventually cause the above false positive error as well. It is advised in this case to use a different `testbed_id` to ensure that
there is no overlap between instances.

### I am getting an error "Not enough instances: missing X instances" when running a benchmark

In the common case to successfully run a benchmark we need to have enough instances available to run

- the required validators
- the grafana dashboard
- the benchmarking clients

for example when running the command `cargo run --bin sui-aws-orchestrator -- benchmark --committee 4 fixed-load --loads 500 --duration 500`, we'll need the following amount of instances available:

- `4 instances` to run the validators (since we set `--committee 4`)
- `1 instance` to run the grafana dashboard (by default only 1 is needed)
- no additional instances to run the benchmarking clients, as those will be co-deployed on the validator nodes

so in total we must have deployed a testbed of at least `5 instances`. If we attempt to run with fewer, then the above error will be thrown.
