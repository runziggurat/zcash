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

## Configuration

Ziggurat is configured with a `config.toml` file in the root.

```toml
kind = "zebra"
path = "path/to/zebra/repo"
start_command = "cargo +stable r -- --config zebra.toml --verbose start"

# kind = "zcashd"
# path = "path/to/zcash/repo"
# start_command = "./src/zcashd -debug=1 -dnsseed=0 -printtoconsole -logips=1 -listenonion=0 -dns=0 -conf=/path/to/zcash/repo/zcash.conf"
```

Information about the node to be tested can be set under the `[node]` table:

- `kind`: one of `zebra` or `zcashd`
- `path`: absolute path in which to run the start and stop commands.
- `start_command`: the command used to start the node (args inline).

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
| [![conf_009](https://img.shields.io/badge/009-██████████-green)   ](SPEC.md#ZG-CONFORMANCE-009)|
| [![conf_010](https://img.shields.io/badge/010-██████████-red)     ](SPEC.md#ZG-CONFORMANCE-010)|
| [![conf_011](https://img.shields.io/badge/011-░░░░░░░░░░-inactive)](SPEC.md#ZG-CONFORMANCE-011)|
| [![conf_012](https://img.shields.io/badge/012-██████████-red)     ](SPEC.md#ZG-CONFORMANCE-012)| :warning: Zcashd node config requires investigation.
| [![conf_013](https://img.shields.io/badge/013-██████████-red)     ](SPEC.md#ZG-CONFORMANCE-013)| :warning: Need to confirm expected behaviour with zcash.
| [![conf_014](https://img.shields.io/badge/014-░░░░░░░░░░-inactive)](SPEC.md#ZG-CONFORMANCE-014)|
| [![conf_015](https://img.shields.io/badge/015-██████████-red)     ](SPEC.md#ZG-CONFORMANCE-015)|
| [![conf_016](https://img.shields.io/badge/016-██████████-red)     ](SPEC.md#ZG-CONFORMANCE-016)|
| [![conf_017](https://img.shields.io/badge/017-█████░░░░░-red)     ](SPEC.md#ZG-CONFORMANCE-017)| Tx portion still todo

| Performance | Additional Information |
| :---------- | :--------------------- |
| [![perf_001](https://img.shields.io/badge/001-░░░░░░░░░░-inactive)](SPEC.md#ZG-PERFORMANCE-001)|
| [![perf_002](https://img.shields.io/badge/002-░░░░░░░░░░-inactive)](SPEC.md#ZG-PERFORMANCE-002)|

| Resistance | Additional Information |
| :--------- | :--------------------- |
| [![resis_001](https://img.shields.io/badge/001-████████░░-red)](SPEC.md#ZG-RESISTANCE-001)| :warning: Need to confirm expected behaviour with zcash. Message specific fuzzing isn't implemented for all messages yet.
| [![resis_002](https://img.shields.io/badge/002-████████░░-red)](SPEC.md#ZG-RESISTANCE-002)| :warning: Need to confirm expected behaviour with zcash. Message specific fuzzing isn't implemented for all messages yet.
| [![resis_003](https://img.shields.io/badge/003-░░░░░░░░░░-inactive)](SPEC.md#ZG-RESISTANCE-003)|
| [![resis_004](https://img.shields.io/badge/004-░░░░░░░░░░-inactive)](SPEC.md#ZG-RESISTANCE-004)|
| [![resis_005](https://img.shields.io/badge/005-████████░░-red)](SPEC.md#ZG-RESISTANCE-005)| :warning: Need to confirm expected behaviour with zcash. Message specific fuzzing isn't implemented for all messages yet.
| [![resis_006](https://img.shields.io/badge/006-░░░░░░░░░░-inactive)](SPEC.md#ZG-RESISTANCE-006)|
