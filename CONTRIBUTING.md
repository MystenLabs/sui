# Contribution Guide

Our goal is to make contributing to the Libra and Calibra projects easy and transparent.

<blockquote class="block_note">
The repository Calibra Research is meant to share content related to the research projects of Calibra.
</blockquote>

## Contributing to Calibra Research

To contribute, ensure that you have the latest version of the codebase. To clone the repository, run the following:
```bash
$ git clone https://github.com/calibra/research.git calibra-research
$ cd calibra-research
$ cargo build --all --all-targets
$ cargo test
```

## Coding Guidelines for Rust code

For detailed guidance on how to contribute to the Rust code in the Calibra Research repository refer to [Coding Guidelines](https://developers.libra.org/docs/coding-guidelines).

## Pull Requests

Please refer to the documentation to determine the status of each project (e.g. actively developed vs. archived) before submitting a pull request.

To submit your pull request:

1. Fork Calibra's `research` repository and create your branch from `master`.
2. If you have added code that should be tested, add unit tests.
3. If you have made changes to APIs, update the relevant documentation, and build and test the developer site.
4. Verify and ensure that the test suite passes.
5. Make sure your code passes both linters.
6. Complete the Contributor License Agreement (CLA), if you haven't already done so.
7. Submit your pull request.

## Contributor License Agreement

For your pull requests to be accepted by any Libra and Calibra project, you will need to sign a CLA. You will need to do this only once to work on any Libra open source project. Individuals contributing on their own behalf can sign the [Individual CLA](https://github.com/libra/libra/blob/master/contributing/individual-cla.pdf). If you are contributing on behalf of your employer, please ask them to sign the [Corporate CLA](https://github.com/libra/libra/blob/master/contributing/corporate-cla.pdf).

## Code of Conduct
Please refer to the [Code of Conduct](https://github.com/libra/libra/blob/master/CODE_OF_CONDUCT.md) for guidelines on interacting with the community.

## Issues

Calibra uses [GitHub issues](https://github.com/calibra/research/issues) to track bugs. Please include necessary information and instructions to reproduce your issue.
