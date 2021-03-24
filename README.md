# Ziggurat

Ziggurat is network test suite that provides zcashd and zebra devs with a reliable foundation of conformance, performance, and resistance.

A Ziggurat is a rectangular stepped tower that uses precise measurements to ensure that each foundation can support the layers above. This metaphor can be applied to network testing by defining three layers:

1. Conformance - adhering to the network protocol
2. Performance - healthy throughput under pressure
3. Resistance - avoiding malicious behavior

## Configuration

Ziggurat is configured with a `config.toml` file in the root which is used to set information about the node to be tested. It must contain:

- `kind`: one of `zebra` or `zcashd`
- `path`: absolute path in which to run the start and stop commands.
- `start_command`: the command used to start the node (args inline).

and optionally:

- `stop_command`: the command used to stop the node. This may be useful when running e.g. a dockerised instance of the node.

When starting the node, this information and the configuration provided in the tests will be written to a configuration file compatible with and read by the node under test.

## Running the Tests

Ziggurat currently uses rust's standard test runner, a simple `cargo test` should suffice.
