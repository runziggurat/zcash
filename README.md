# Ziggurat
> The Zcash Network Stability Framework

<img src="./logo.png" alt="Ziggurat Logo" width="240" />

A Ziggurat is a rectangular stepped tower that uses precise measurements to ensure
that each foundation can support the layers above. This metaphor can be applied to
network testing by defining three layers:

1. Conformance - adhering to the network protocol
2. Performance - healthy throughput under pressure
3. Resistance - avoiding malicious behavior

Ziggurat is network test suite that provides [zcashd](https://github.com/ZcashFoundation/zcashd)
and [zebra](https://github.com/ZcashFoundation/zebra) devs with this reliable foundation.

*Note:* This project is a work in progress.

## Configuration

Ziggurat is configured with a `config.toml` file in the root.

```toml
[node]
kind = "zebra"
path = "path/to/zebra/repo"
start_command = "cargo +stable r -- --config node.toml --verbose start"

# For dockerized zcashd:
# local_ip = "0.0.0.0"
# [node]
# kind = "zcashd"
# path = "/path/to/zcashd/configfile/usually/home/dir"
# start_command = "docker start my_zcashd"
# stop_command = "docker stop my_zcashd"
# local_addr = "0.0.0.0:8080"
# external_addr = "0.0.0.0:8080"
# peer_ip = "host.docker.internal"
```

The networking properties of Ziggurat itself can be set with:
- `local_ip`: the local ip to use with all Ziggurat spawned listeners. Defaults to localhost.

Additionally, information about the node to be tested can be set under the `[node]` table:

- `kind`: one of `zebra` or `zcashd`
- `path`: absolute path in which to run the start and stop commands, or your zcashd config in the case of dockerized zcashd.
- `start_command`: the command used to start the node (args inline).

and optionally:

- `stop_command`: the command used to stop the node. This may be useful when running e.g. a dockerised instance of the node.
- `local_addr`: the local address of the node. Defaults to localhost, should be set if the node needs distinct local and external addresses.
- `external_addr`: the external address of the node. Defaults to localhost.
- `peer_ip`: the ip/dns name the node can reach the peers through.


When starting the node, this information and the configuration provided in the tests will be written to a configuration file compatible with and read by the node under test.

## Running the Tests

Ziggurat currently uses rust's standard test runner, a simple `cargo test -- --test-threads=1` should suffice. We use the single threaded executor as spinning up multiple test nodes isn't currently supported.

## Project Status

Quick overview of the current status, providing implementation progress and test pass / fail state.

|:exclamation: Note that test completion is **not** indicative of remaining effort, but rather what percentage of the test case is covered.|
|---|

| Conformance | Additional Information |
| :---------- | :--------------------- |
| [![conf_001](https://img.shields.io/badge/001-██████████-green)   ](SPEC.md#ZG-CONFORMANCE-001)|
| [![conf_001](https://img.shields.io/badge/002-██████████-green)   ](SPEC.md#ZG-CONFORMANCE-002)|
| [![conf_003](https://img.shields.io/badge/003-██████████-green)   ](SPEC.md#ZG-CONFORMANCE-003)| :warning: Need to confirm expected behaviour with zcash.
| [![conf_004](https://img.shields.io/badge/004-██████░░░░-green)   ](SPEC.md#ZG-CONFORMANCE-004)| :warning: Need to confirm expected behaviour with zcash.
| [![conf_005](https://img.shields.io/badge/005-█████░░░░░-red)     ](SPEC.md#ZG-CONFORMANCE-005)| :warning: Need to confirm expected behaviour with zcash.
| [![conf_006](https://img.shields.io/badge/006-██████████-green)   ](SPEC.md#ZG-CONFORMANCE-006)| :warning: Need to confirm expected behaviour with zcash.
| [![conf_007](https://img.shields.io/badge/007-██████████-red)     ](SPEC.md#ZG-CONFORMANCE-007)| :warning: Need to confirm expected behaviour with zcash.
| [![conf_008](https://img.shields.io/badge/008-████░░░░░░-red)     ](SPEC.md#ZG-CONFORMANCE-008)| :warning: Need to confirm expected behaviour with zcash.
| [![conf_009](https://img.shields.io/badge/009-██████░░░░-green)   ](SPEC.md#ZG-CONFORMANCE-009)|
| [![conf_010](https://img.shields.io/badge/010-░░░░░░░░░░-inactive)](SPEC.md#ZG-CONFORMANCE-010)|
| [![conf_011](https://img.shields.io/badge/011-░░░░░░░░░░-inactive)](SPEC.md#ZG-CONFORMANCE-011)|
| [![conf_012](https://img.shields.io/badge/012-░░░░░░░░░░-inactive)](SPEC.md#ZG-CONFORMANCE-012)|
| [![conf_013](https://img.shields.io/badge/013-░░░░░░░░░░-inactive)](SPEC.md#ZG-CONFORMANCE-013)|
| [![conf_014](https://img.shields.io/badge/014-░░░░░░░░░░-inactive)](SPEC.md#ZG-CONFORMANCE-014)|
| [![conf_015](https://img.shields.io/badge/015-░░░░░░░░░░-inactive)](SPEC.md#ZG-CONFORMANCE-015)|
| [![conf_016](https://img.shields.io/badge/016-░░░░░░░░░░-inactive)](SPEC.md#ZG-CONFORMANCE-016)|
| [![conf_017](https://img.shields.io/badge/017-░░░░░░░░░░-inactive)](SPEC.md#ZG-CONFORMANCE-017)|

| Performance | Additional Information |
| :---------- | :--------------------- |
| [![perf_001](https://img.shields.io/badge/001-░░░░░░░░░░-inactive)](SPEC.md#ZG-PERFORMANCE-001)|
| [![perf_002](https://img.shields.io/badge/002-░░░░░░░░░░-inactive)](SPEC.md#ZG-PERFORMANCE-002)|

| Resistance | Additional Information |
| :--------- | :--------------------- |
| [![resis_001](https://img.shields.io/badge/001-░░░░░░░░░░-inactive)](SPEC.md#ZG-RESISTANCE-001)|
| [![resis_002](https://img.shields.io/badge/002-░░░░░░░░░░-inactive)](SPEC.md#ZG-RESISTANCE-002)|
| [![resis_003](https://img.shields.io/badge/003-░░░░░░░░░░-inactive)](SPEC.md#ZG-RESISTANCE-003)|
| [![resis_004](https://img.shields.io/badge/004-░░░░░░░░░░-inactive)](SPEC.md#ZG-RESISTANCE-004)|
| [![resis_005](https://img.shields.io/badge/005-░░░░░░░░░░-inactive)](SPEC.md#ZG-RESISTANCE-005)|
| [![resis_006](https://img.shields.io/badge/006-░░░░░░░░░░-inactive)](SPEC.md#ZG-RESISTANCE-006)|
