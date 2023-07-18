# Orchestrator

The Orchestrator crate provides facilities for quickly deploying and benchmarking this codebase in a geo-distributed environment. Please note that it is not intended for production deployments or as an indicator of production engineering best practices. Its purpose is to facilitate research projects by allowing benchmarking of (variants of) the codebase and analyzing performance.

This guide provides a step-by-step explanation of how to run geo-distributed benchmarks on either [Vultr](http://vultr.com) or [Amazon Web Services (AWS)](http://aws.amazon.com).

## Step 1. Set up cloud provider credentials

To enable programmatic access to your cloud provider account from your local machine, you need to set up your cloud provider credentials. These credentials authorize your machine to create, delete, and edit instances programmatically on your account.

### Setting up Vultr credentials

1. Find your ['Vultr token'](https://www.vultr.com/docs/).
2. Create a file `~/.vultr` and add your access token as the file's content:

```text
YOUR_ACCESS_TOKEN
```

### Setting up AWS credentials

1. Find your ['access key id' and 'secret access key'](https://docs.aws.amazon.com/cli/latest/userguide/cli-configure-quickstart.html#cli-configure-quickstart-creds).
2. Create a file `~/.aws/credentials` with the following content:

```text
[default]
aws_access_key_id = YOUR_ACCESS_KEY_ID
aws_secret_access_key = YOUR_SECRET_ACCESS_KEY
```

Do not specify any AWS region in that file, as the scripts need to handle multiple regions programmatically.

## Step 2. Specify the testbed configuration

Create a file called `settings.json` that contains all the configuration parameters for the testbed deployment. You can find an example file at `./assets/settings.json` with the following content:

```json
{
  "testbed_id": "alberto-mysticeti",
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
    "url": "http://github.com/mystenlabs/project-mysticeti.git",
    "commit": "orchestrator"
  },
  "results_directory": "./results",
  "logs_directory": "./logs"
}
```

The documentation of the `Settings` struct in `./src/settings.rs` provides detailed information about each field and indicates which ones are optional. If you're working with a private GitHub repository, you can include a [private access token](https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/managing-your-personal-access-tokens) in the repository URL. For example, if your access token is `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`, the repository URL should be formatted as follows:

```json
"repository": {
  "url": "http://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@github.com/mystenlabs/project-mysticeti.git",
  "commit": "orchestrator"
}
```

## Step 3. Create a testbed

The `orchestrator` binary provides various functionalities for creating, starting, stopping, and destroying instances. You can use the following command to boot 2 instances per region (if the settings file specifies 10 regions, as shown in the example above, a total of 20 instances will be created):

```bash
cargo run --bin orchestrator -- testbed deploy --instances 2
```

To check the current status of the testbed instances, use the following command:

```bash
cargo run --bin orchestrator testbed status
```

Instances listed with a green number are available and ready for use, while instances listed with a red number are stopped.

## Step 4. Running benchmarks

Running benchmarks involves installing the specified version of the codebase on the remote machines and running one validator and one load generator per instance. For example, the following command benchmarks a committee of 10 validators under a constant load of 200 tx/s for 3 minutes:

```bash
cargo run --bin orchestrator -- benchmark --committee 10 fixed-load --loads 200 --duration 180
```

In a network of 10 validators, each with a corresponding load generator, each load generator submits a fixed load of 20 tx/s. Performance measurements are collected by regularly scraping the Prometheus metrics exposed by the load generators. The `orchestrator` binary provides additional commands to run a specific number of load generators on separate machines.

## Step 5. Monitoring

The orchestrator provides facilities to monitor metrics on clients and nodes. When run with the flab `--monitor`, the orchestrator deploys a [Prometheus](https://prometheus.io) instance and a [Grafana](https://grafana.com) instance on a dedicated remote machine. Grafana is then available on the address printed on stdout (e.g., `http://3.83.97.12:3000`) with the default username and password both set to `admin`. You can either create a [new dashboard](https://grafana.com/docs/grafana/latest/getting-started/build-first-dashboard/) or [import](https://grafana.com/docs/grafana/latest/dashboards/manage-dashboards/#import-a-dashboard) the example dashboard located in the `./assets` folder.
