# Ziggurat CI/CD

This documentation details information on how this implementation handles CI/CD, and can be used as a reference for setting up your own CI/CD pipeline with Ziggurat. Currently the Ziggurat CI/CD pipeline includes three concurrent workflows that run daily, these are the test suites for `zcashd` and `zebra`, and the network crawler.

## Test Suite

The test suite workflows can be broken down into the following 5 steps:
1. Build a selected node from source.
2. Compile Ziggurat unit tests.
3. Create the Ziggurat config file. 
4. Run the Ziggurat tests executable.
5. Process the results.

## Network Crawler

The network crawler workflow can be broken down into the following 4 steps:
1. Build a `zcashd` node from source.
2. Run the crawler binary with the compiled node as the network entry point.
3. Wait 30 minutes, then query metrics via RPC and kill the running crawler.
4. Process the results.

Details on how to run the crawler, including the required arguments and how to work with the RPC, can be found [here](../../src/tools/crawler/README.md).

## Workflow References

- [Test Suite (`zcashd`)](./zcashd-nightly.yml)
- [Test Suite (`zebra`)](./zebra.yml)
- [Network Crawler](./crawler.yml)
- [Build `zcashd`](./build-zcashd.yml)

### Ziggurat Core Workflows

Most workflows will also reference a set of core utilities that are used throughout the Ziggurat ecosystem. These can all be found in the `ziggurat-core` repository, which can be found [here](https://github.com/runziggurat/ziggurat-core/blob/main/.github/workflows/README.md).
