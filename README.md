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

## Test Status

### Conformance

| Test Case                         | ZCashd | Zebra | Additional Information |
| :-------------------------------: | :----: | :---: | :--------------------- |
| [001](SPEC.md#ZG-CONFORMANCE-001) |   ✓    |   ✓   |
| [002](SPEC.md#ZG-CONFORMANCE-002) |   ✓    |   ✓   |
| [003](SPEC.md#ZG-CONFORMANCE-003) |   ✓    |   ✓   |
| [004](SPEC.md#ZG-CONFORMANCE-004) |   ✓    |   ✓   |
| [005](SPEC.md#ZG-CONFORMANCE-005) |   ✖    |   ✖   |
| [006](SPEC.md#ZG-CONFORMANCE-006) |   ✓    |   ✓   |
| [007](SPEC.md#ZG-CONFORMANCE-007) |   ✖    |   ✖   |
| [008](SPEC.md#ZG-CONFORMANCE-008) |   ✖    |   ✖   | ⚠ filter's may need work (malformed), ⚠ require zcashd feedback
| [009](SPEC.md#ZG-CONFORMANCE-009) |   ✓    |   ✓   |
| [010](SPEC.md#ZG-CONFORMANCE-010) |   ✖    |   ✖   | ⚠ todo: mempool seeding
| [011](SPEC.md#ZG-CONFORMANCE-011) |   ✖    |   ✖   |
| [012](SPEC.md#ZG-CONFORMANCE-012) |   ✖    |   ✖   | ⚠ zcashd peering issues
| [013](SPEC.md#ZG-CONFORMANCE-013) |   ✖    |   ✖   | ⚠ zcashd peering issues
| [014](SPEC.md#ZG-CONFORMANCE-014) |   -    |   -   | ⚠ Not yet implemented (blocked by mempool seeding)
| [015](SPEC.md#ZG-CONFORMANCE-015) |   ✓    |   -   | ⚠ todo: zebra block seeding
| [016](SPEC.md#ZG-CONFORMANCE-016) |   ✖    |   -   | ⚠ todo: zebra block seeding
| [017](SPEC.md#ZG-CONFORMANCE-017) |   ✓    |   ✖   | ⚠ partially implemented (requires mempool seeding, and zebra block seeding)

### Performance

| Test Case                         | ZCashd | Zebra | Additional Information |
| :-------------------------------: | :----: | :---: | :--------------------- |
| [001](SPEC.md#ZG-PERFORMANCE-001) |   ✓    |   ✖   |
| [002](SPEC.md#ZG-PERFORMANCE-002) |   ✓    |   ✖   |

### Resistance: fuzzing zeros

| Test Case                         | ZCashd | Zebra | Additional Information |
| :-------------------------------: | :----: | :---: | :--------------------- |
| [001](SPEC.md#ZG-RESISTANCE-001)  |   ✓    |   ✓   |
| [002](SPEC.md#ZG-RESISTANCE-002)  |   ✓    |   ✓   |
| [003](SPEC.md#ZG-RESISTANCE-003)  |   ✓    |   ✓   | Zcashd is extremely slow
| [004](SPEC.md#ZG-RESISTANCE-004)  |   ✓    |   ✓   | Zcashd is extremely slow
| [005](SPEC.md#ZG-RESISTANCE-005)  |   ✓    |   ✓   |
| [006](SPEC.md#ZG-RESISTANCE-006)  |   -    |   -   | ⚠ Not yet implemented

### Resistance: fuzzing random bytes

| Test Case                         | ZCashd | Zebra | Additional Information |
| :-------------------------------: | :----: | :---: | :--------------------- |
| [001](SPEC.md#ZG-RESISTANCE-001)  |   ✓    |   ✓   |
| [002](SPEC.md#ZG-RESISTANCE-002)  |   ✓    |   ✓   |
| [003](SPEC.md#ZG-RESISTANCE-003)  |   ✓    |   ✓   | Zcashd is extremely slow
| [004](SPEC.md#ZG-RESISTANCE-004)  |   ✓    |   ✓   | Zcashd is extremely slow
| [005](SPEC.md#ZG-RESISTANCE-005)  |   ✓    |   ✓   |
| [006](SPEC.md#ZG-RESISTANCE-006)  |   -    |   -   | ⚠ Not yet implemented

### Resistance: fuzzing random payloads

| Test Case                         | ZCashd | Zebra | Additional Information |
| :-------------------------------: | :----: | :---: | :--------------------- |
| [001](SPEC.md#ZG-RESISTANCE-001)  |   ✖    |   ✖   |
| [002](SPEC.md#ZG-RESISTANCE-002)  |   ✖    |   ✖   |
| [003](SPEC.md#ZG-RESISTANCE-003)  |   ✖    |   ✖   | Zcashd is extremely slow
| [004](SPEC.md#ZG-RESISTANCE-004)  |   ✖    |   ✖   | Zcashd is extremely slow
| [005](SPEC.md#ZG-RESISTANCE-005)  |   ✓    |   ✓   |
| [006](SPEC.md#ZG-RESISTANCE-006)  |   -    |   -   | ⚠ Not yet implemented

### Resistance: fuzzing corrupt messages

| Test Case                         | ZCashd | Zebra | Additional Information |
| :-------------------------------: | :----: | :---: | :--------------------- |
| [001](SPEC.md#ZG-RESISTANCE-001)  |   ✖    |   ✓   |
| [002](SPEC.md#ZG-RESISTANCE-002)  |   ✖    |   ✓   |
| [003](SPEC.md#ZG-RESISTANCE-003)  |   ✖    |   ✓   |
| [004](SPEC.md#ZG-RESISTANCE-004)  |   ✖    |   ✓   |
| [005](SPEC.md#ZG-RESISTANCE-005)  |   ✖    |   ✖   |
| [006](SPEC.md#ZG-RESISTANCE-006)  |   -    |   -   | ⚠ Not yet implemented

### Resistance: fuzzing corrupt checksum

| Test Case                         | ZCashd | Zebra | Additional Information |
| :-------------------------------: | :----: | :---: | :--------------------- |
| [001](SPEC.md#ZG-RESISTANCE-001)  |   ✖    |   ✓   |
| [002](SPEC.md#ZG-RESISTANCE-002)  |   ✖    |   ✓   |
| [003](SPEC.md#ZG-RESISTANCE-003)  |   ✖    |   ✓   |
| [004](SPEC.md#ZG-RESISTANCE-004)  |   ✖    |   ✓   |
| [005](SPEC.md#ZG-RESISTANCE-005)  |   ✖    |   ✓   |
| [006](SPEC.md#ZG-RESISTANCE-006)  |   -    |   -   | ⚠ Not yet implemented

### Resistance: fuzzing corrupt length

| Test Case                         | ZCashd | Zebra | Additional Information |
| :-------------------------------: | :----: | :---: | :--------------------- |
| [001](SPEC.md#ZG-RESISTANCE-001)  |   ✓    |   ✓   |
| [002](SPEC.md#ZG-RESISTANCE-002)  |   ✓    |   ✓   |
| [003](SPEC.md#ZG-RESISTANCE-003)  |   ✓    |   ✓   | Zcashd is extremely slow
| [004](SPEC.md#ZG-RESISTANCE-004)  |   ✓    |   ✓   | Zcashd is extremely slow
| [005](SPEC.md#ZG-RESISTANCE-005)  |   ✓    |   ✓   |
| [006](SPEC.md#ZG-RESISTANCE-006)  |   -    |   -   | ⚠ Not yet implemented