### Capturing log output in tests
1. Add this call at the top of the test
   ```
   telemetry_subscribers::init_for_testing();
   ```
2. Run the test with the `--nocapture` (goes after `--`) command line flag (see https://doc.rust-lang.org/cargo/commands/cargo-test.html#display-options).
3. Optionally set the `RUST_LOG` (example `RUST_LOG=debug`) environment variable (defaults to `info`).

### Debugging transactional tests in RustRover

1. Add new `Cargo` run configuration
2. 1. Command to run all tests 
      ```
      test -p sui-indexer-alt-e2e-tests --test transactional_tests
      ```
   2. Command to run single test (replace `graphql/epochs/query.move` with the test you want to run)
      ```
      test -p sui-indexer-alt-e2e-tests --test transactional_tests graphql/epochs/query.move
      ```
   3. Note: debugging is not supported for nextest (https://youtrack.jetbrains.com/issue/RUST-12459)
3. To prevent `error: invalid value 'json' for '--format <pretty|terse|json>'` or `error: unexpected argument '-Z' found`, uncheck 
   
   ```
   RustRover -> Settings -> Advanced Settings -> Show test results in the Test tool window
   ```
   
   (https://youtrack.jetbrains.com/issue/RUST-7891/Cargo-test-bench-something-fails-with-unexpected-argument#focus=Comments-27-8478362.0-0)
4. To prevent `FATAL:  postmaster became multithreaded during startup` in homebrew postgres15 server logs, 
   set `Environment Variables` to `LC_ALL=en_US.UTF-8`.

   (https://github.com/Homebrew/homebrew-core/issues/124215#issuecomment-1445300937)