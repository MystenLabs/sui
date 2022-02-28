## Contributing to Sui

TODO: Define basic system requirements for a reliable environment: recommended OS and required packages.

To contribute, ensure you have the latest version of the codebase. To clone the repository, run the following:
```bash
git clone https://github.com/mystenlabs/fastnft.git
cd sui
cargo build --all --all-targets
cargo test
```
TODO: Note the `git clone` command above fails with the following error, which *should* go away when we open our repo up:

```
remote: Support for password authentication was removed on August 13, 2021. Please use a personal access token instead.
remote: Please see https://github.blog/2020-12-15-token-authentication-requirements-for-git-operations/ for more information.
```

## Coding guidelines for Rust code

For detailed guidance on how to contribute to the Rust code in the Sui repository refer to the [Diem developer documentation](https://developers.diem.com/docs/welcome-to-diem/).

## Pull requests

Please refer to the documentation to determine the status of each project (e.g. actively developed vs. archived) before submitting a pull request.

To submit your pull request:

1. Fork the `sui` repository and create your branch from `main`.
2. If you have added code that should be tested, add unit tests.
3. If you have made changes to APIs, update the relevant documentation, and build and test the developer site.
4. Verify and ensure that the test suite passes.
5. Make sure your code passes both linters.
6. Complete the Contributor License Agreement (CLA), if you haven't already done so.
7. Submit your pull request.

TODO: Add links to the steps above for more details, such as how to build and test the dev site and where to find the CLA.

## Code of Conduct
Please refer to the [Diem Code of Conduct](https://developers.diem.com/docs/policies/code-of-conduct/) for guidelines on interacting with the community.
