# Ziggurat
> The Zcash Network Stability Framework

<img src="./logo.png" alt="Ziggurat Logo" width="240" />

A Ziggurat is a rectangular stepped tower that uses precise measurements to ensure
that each foundation can support the layers above. This metaphor can be applied to
network testing by defining three layers:

1. Conformance - adhering to the network protocol
2. Performance - healthy throughput under pressure
3. Resistance - avoiding malicious behavior

Ziggurat is network test suite that provides [zcashd](https://github.com/zcash/zcash)
and [zebra](https://github.com/ZcashFoundation/zebra) devs with this reliable foundation.

*Note:* This project is a work in progress.

## Prerequisites

Ziggurat is written in stable Rust; you can install the Rust toolchain by following the official instructions [here](*https://www.rust-lang.org/learn/get-started).

You also need to install at least one node implementation to test. Ziggurat is currently configured to test the Nu5 network protocol (version `170_015`).

### Zcashd

`zcashd` can be installed by using the [official instructions](https://zcash.readthedocs.io/en/latest/rtd_pages/zcashd.html) for your operating system. We recommend building from source for consistency and to ensure you're using the right versions. Alternatively, you can use ECC's Debian/Ubuntu package or the binary tarball.

However, **please note that** **Docker is not supported** as it can theoretically produce unreliable test results and increases network complexity.

```bash
# After installing dependencies
$ git clone https://github.com/zcash/zcash
$ cd zcash
$ git checkout v4.4.1            # optional, or use master
$ ./zcutil/fetch-params.sh
$ ./zcutil/clean.sh
$ ./zcutil/build.sh -j$(nproc)   # or number of cores
```

After completing the above, you can skip the configuration steps, i.e. creating `~/.zcashd/zcash.conf` as Ziggurat will create new configuration files for each test run. Also, syncing the blockchain is not required.

### Zebra

`zebra` can be installed from its [source code](https://github.com/ZcashFoundation/zebra) on GitHub. Although a Dockerfile is provided, **Docker is not supported.** We suggest following the instructions below, or similar.

```bash
# After installing dependencies
$ git clone https://github.com/ZcashFoundation/zebra
$ cd zebra
# Download the parameters
$ cargo +stable run --release download
# Run the node once before testing with Ziggurat, just to make sure all is working correctly
$ cargo +stable run --release -- --verbose start
```

Similarly to `zcashd`, configuration is not necessary since Ziggurat generates new configurations for each test run.

## Configuration

Ziggurat is configured via a `config.toml` file in the `~/.ziggurat` directory (you'll need to create this yourself). It must contain the following fields:

- `kind`: one of `zebra` or `zcashd`.
- `path`: absolute path in which to run the start command.
- `start_command`: the command used to start the node

We recommend using the following Zcashd config:
```toml
kind = "zcashd"
path = "path/to/zcash/repo"
start_command = "./src/zcashd -debug=1 -printtoconsole -logips=1 -dnsseed=0 -dns=0 -listenonion=0"
# debug=1           enables debug logging
# logips=1          adds connection IP spans to the logs
# printtoconsole    logs to stdout
# dnsseed=0         disables looking for hardcoded DNS seeding nodes (we want to isolate our node to just the test)
# dns=0             disables DNS lookup
# listenonion=0     disables the Tor network
```
and for Zebra:
```toml
kind = "zebra"
path = "path/to/zebra/repo"
start_command = "cargo +stable r -- --verbose start"
# cargo +stable r   run Zebra using stable Rust
# --                all args after this will get passed to Zebra
# verbose           enables verbose logging
# start             starts the node
```

| :warning: Zcashd: `-datadir` |
| :------------------------------|
| Ziggurat uses the `-datadir` configuration argument internally for Zcashd nodes, to prevent corrupting the user's Zcashd cache. This option gets appended to the start command, and will override any user specified `-datadir` values.|

## Building the docs

Ziggurat's documentation can be built with `cargo doc --no-deps --open`.

## Running the Tests

Ziggurat currently uses rust's standard test runner, a simple `cargo test -- --test-threads=1` should suffice. We use the single threaded executor as spinning up multiple test nodes isn't currently supported.

### Logging

Logs are disabled by default, as they usually just add noise and slow down the test. They can be very useful for debugging and can be enabled on a test case level.

Ziggurat's `SyntheticNode` supports `tracing` - this can be enabled by inserting a call to `synthetic_node::enable_tracing()` inside the test case.

The test node's `stdout` and `stderr` logs can be piped to `stdout` by inserting a call to `node.log_to_stdout(true)` before starting the node. Note that logs will need to be enabled for the node as detailed in [Configuration](#Configuration).

```Rust
let mut node = Node::new().unwrap();
node.initial_action(Action::WaitForConnection)
    .log_to_stdout(true)    // pipes the node's `stdout` and `stderr` to `stdout`
    .start()
    .await
    .unwrap();
```

## Test Status

Short overview of test cases and their current status. In case of failure, the behaviour observed for `zebra` and `zcashd` is usually documented in the test case.

These results were obtained by running the test suite against [Zcashd v4.4.1](https://github.com/zcash/zcash/releases/tag/v4.4.1) (0dade79ce) and [Zebra 1.0.0-alpha.11](https://github.com/ZcashFoundation/zebra/releases/tag/v1.0.0-alpha.11) (6396ac2).

| Legend |               |
| :----: | ------------- |
|   ✓    | pass          |
|   ✖    | fail          |
|   -    | unimplemented |

### Conformance

|             Test Case             | Zcashd | Zebra | Additional Information                                                      |
| :-------------------------------: | :----: | :---: | :-------------------------------------------------------------------------- |
| [001](SPEC.md#ZG-CONFORMANCE-001) |   ✓    |   ✓   |                                                                             |
| [002](SPEC.md#ZG-CONFORMANCE-002) |   ✓    |   ✓   |                                                                             |
| [003](SPEC.md#ZG-CONFORMANCE-003) |   ✓    |   ✖   |                                                                             |
| [004](SPEC.md#ZG-CONFORMANCE-004) |   ✓    |   ✖   |                                                                             |
| [005](SPEC.md#ZG-CONFORMANCE-005) |   ✓    |   ✖   |                                                                             |
| [006](SPEC.md#ZG-CONFORMANCE-006) |   ✓    |   ✓   |                                                                             |
| [007](SPEC.md#ZG-CONFORMANCE-007) |   ✓    |   ✓   |                                                                             |
| [008](SPEC.md#ZG-CONFORMANCE-008) |   ✓    |   ✖   |                                                                             |
| [009](SPEC.md#ZG-CONFORMANCE-009) |   ✖    |   ✖   | ⚠ filters may need work (malformed), ⚠ require zcashd feedback              |
| [010](SPEC.md#ZG-CONFORMANCE-010) |   ✓    |   ✓   |                                                                             |
| [011](SPEC.md#ZG-CONFORMANCE-011) |   ✖    |   ✖   | ⚠ todo: mempool seeding                                                     |
| [012](SPEC.md#ZG-CONFORMANCE-012) |   ✖    |   ✖   |                                                                             |
| [013](SPEC.md#ZG-CONFORMANCE-013) |   ✖    |   ✖   | ⚠ zcashd peering issues, zebra passes under certain conditions              |
| [014](SPEC.md#ZG-CONFORMANCE-014) |   ✖    |   ✖   | ⚠ zcashd peering issues                                                     |
| [015](SPEC.md#ZG-CONFORMANCE-015) |   -    |   -   | ⚠ not yet implemented (blocked by mempool seeding)                          |
| [016](SPEC.md#ZG-CONFORMANCE-016) |   ✖    |   -   | ⚠ todo: zebra block seeding                                                 |
| [017](SPEC.md#ZG-CONFORMANCE-017) |   ✖    |   -   | ⚠ todo: zebra block seeding                                                 |
| [018](SPEC.md#ZG-CONFORMANCE-018) |   ✖    |   ✖   | ⚠ partially implemented (requires mempool seeding, and zebra block seeding) |

### Performance

|             Test Case             | Zcashd | Zebra | Additional Information |
| :-------------------------------: | :----: | :---: | :--------------------- |
| [001](SPEC.md#ZG-PERFORMANCE-001) |   ✓    |   ✖   |                        |
| [002](SPEC.md#ZG-PERFORMANCE-002) |   ✓    |   ✖   |                        |

### Resistance: fuzzing zeros

|            Test Case             | Zcashd | Zebra | Additional Information   |
| :------------------------------: | :----: | :---: | :----------------------- |
| [001](SPEC.md#ZG-RESISTANCE-001) |   ✓    |   ✓   |                          |
| [002](SPEC.md#ZG-RESISTANCE-002) |   ✓    |   ✓   |                          |
| [003](SPEC.md#ZG-RESISTANCE-003) |   ✓    |   ✓   | Zcashd is extremely slow |
| [004](SPEC.md#ZG-RESISTANCE-004) |   ✓    |   ✓   | Zcashd is extremely slow |
| [005](SPEC.md#ZG-RESISTANCE-005) |   ✓    |   ✓   |                          |
| [006](SPEC.md#ZG-RESISTANCE-006) |   ✓    |   -   |                          |

### Resistance: fuzzing random bytes

|            Test Case             | Zcashd | Zebra | Additional Information   |
| :------------------------------: | :----: | :---: | :----------------------- |
| [001](SPEC.md#ZG-RESISTANCE-001) |   ✓    |   ✓   |                          |
| [002](SPEC.md#ZG-RESISTANCE-002) |   ✓    |   ✓   |                          |
| [003](SPEC.md#ZG-RESISTANCE-003) |   ✓    |   ✓   | Zcashd is extremely slow |
| [004](SPEC.md#ZG-RESISTANCE-004) |   ✓    |   ✓   | Zcashd is extremely slow |
| [005](SPEC.md#ZG-RESISTANCE-005) |   ✓    |   ✓   |                          |
| [006](SPEC.md#ZG-RESISTANCE-006) |   ✓    |   -   |                          |

### Resistance: fuzzing random payloads

|            Test Case             | Zcashd | Zebra | Additional Information   |
| :------------------------------: | :----: | :---: | :----------------------- |
| [001](SPEC.md#ZG-RESISTANCE-001) |   ✖    |   ✖   |                          |
| [002](SPEC.md#ZG-RESISTANCE-002) |   ✖    |   ✖   |                          |
| [003](SPEC.md#ZG-RESISTANCE-003) |   ✖    |   ✖   | Zcashd is extremely slow |
| [004](SPEC.md#ZG-RESISTANCE-004) |   ✖    |   ✖   | Zcashd is extremely slow |
| [005](SPEC.md#ZG-RESISTANCE-005) |   ✓    |   ✓   |                          |
| [006](SPEC.md#ZG-RESISTANCE-006) |   ✓    |   -   |                          |

### Resistance: fuzzing corrupt messages

|            Test Case             | Zcashd | Zebra | Additional Information |
| :------------------------------: | :----: | :---: | :--------------------- |
| [001](SPEC.md#ZG-RESISTANCE-001) |   ✖    |   ✓   |                        |
| [002](SPEC.md#ZG-RESISTANCE-002) |   ✖    |   ✓   |                        |
| [003](SPEC.md#ZG-RESISTANCE-003) |   ✖    |   ✓   |                        |
| [004](SPEC.md#ZG-RESISTANCE-004) |   ✖    |   ✓   |                        |
| [005](SPEC.md#ZG-RESISTANCE-005) |   ✖    |   ✖   |                        |
| [006](SPEC.md#ZG-RESISTANCE-006) |   ✓    |   -   |                        |

### Resistance: fuzzing corrupt checksum

|            Test Case             | Zcashd | Zebra | Additional Information |
| :------------------------------: | :----: | :---: | :--------------------- |
| [001](SPEC.md#ZG-RESISTANCE-001) |   ✖    |   ✓   |                        |
| [002](SPEC.md#ZG-RESISTANCE-002) |   ✖    |   ✓   |                        |
| [003](SPEC.md#ZG-RESISTANCE-003) |   ✖    |   ✓   |                        |
| [004](SPEC.md#ZG-RESISTANCE-004) |   ✖    |   ✓   |                        |
| [005](SPEC.md#ZG-RESISTANCE-005) |   ✖    |   ✓   |                        |
| [006](SPEC.md#ZG-RESISTANCE-006) |   ✓    |   -   |                        |

### Resistance: fuzzing corrupt length

|            Test Case             | Zcashd | Zebra | Additional Information   |
| :------------------------------: | :----: | :---: | :----------------------- |
| [001](SPEC.md#ZG-RESISTANCE-001) |   ✓    |   ✓   |                          |
| [002](SPEC.md#ZG-RESISTANCE-002) |   ✓    |   ✓   |                          |
| [003](SPEC.md#ZG-RESISTANCE-003) |   ✓    |   ✓   | Zcashd is extremely slow |
| [004](SPEC.md#ZG-RESISTANCE-004) |   ✓    |   ✓   | Zcashd is extremely slow |
| [005](SPEC.md#ZG-RESISTANCE-005) |   ✓    |   ✓   |                          |
| [006](SPEC.md#ZG-RESISTANCE-006) |   ✓    |   -   |                          |
