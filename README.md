# Ziggurat

This project is a work in progress.

Ziggurat is network test suite that provides zcashd and zebra devs with a reliable foundation of conformance, performance, and resistance.

A Ziggurat is a rectangular stepped tower that uses precise measurements to ensure that each foundation can support the layers above. This metaphor can be applied to network testing by defining three layers:

1. Conformance - adhering to the network protocol
2. Performance - healthy throughput under pressure
3. Resistance - avoiding malicious behavior

## Configuration

Ziggurat is configured with a `config.toml` file in the root. The networking properties of Ziggurat itself can be set with:
- `local_ip`: the local ip to use with all Ziggurat spawned listeners. Defaults to localhost.

Additionally, information about the node to be tested can be set under the `[node]` table:

- `kind`: one of `zebra` or `zcashd`
- `path`: absolute path in which to run the start and stop commands.
- `start_command`: the command used to start the node (args inline).

and optionally:

- `stop_command`: the command used to stop the node. This may be useful when running e.g. a dockerised instance of the node.
- `local_addr`: the local address of the node. Defaults to localhost, should be set if the node needs distinct local and external addresses.
- `external_addr`: the external address of the node. Defaults to localhost.
- `peer_ip`: the ip/dns name the node can reach the peers through.


When starting the node, this information and the configuration provided in the tests will be written to a configuration file compatible with and read by the node under test.

## Running the Tests

Ziggurat currently uses rust's standard test runner, a simple `cargo test -- --test-threads=1` should suffice. We use the single threaded executor as spinning up multiple test nodes isn't currently supported.
