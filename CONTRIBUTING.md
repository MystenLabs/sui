## Contributing to Sui

TODO: Define basic system requirements for a reliable environment: recommended OS and required packages.

To contribute, ensure you have the latest version of the codebase. To clone the repository, run the following:
```bash
git clone https://github.com/mystenlabs/sui.git
cd sui
cargo build --all --all-targets
cargo test
```
TODO: Note the `git clone` command above may fail with the following error, which *should* go away when we open our repo up:

```
remote: Support for password authentication was removed on August 13, 2021. Please use a personal access token instead.
remote: Please see https://github.blog/2020-12-15-token-authentication-requirements-for-git-operations/ for more information.
```

## Pull requests

To submit your pull request:

1. Fork the `sui` repository and create your branch from `main`.
2. If you have added code that should be tested, add unit tests.
3. If you have made changes to APIs, update the relevant documentation, and build and test the developer site.
4. Verify and ensure that the test suite passes.
5. Make sure your code passes both linters.
6. Complete the Contributor License Agreement (CLA), if you haven't already done so.
7. Submit your pull request.

TODO: Add links to the steps above for more details, such as how to build and test the dev site and where to find the CLA once we have one.
