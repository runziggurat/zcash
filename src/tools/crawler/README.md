# Network crawler

## Running

The network crawler uses optional features and dependencies, which **must** be enabled in order for the binary to compile. These can be enabled by supplying `--features crawler` when running the command. 

The following command will print the command line options for the crawler:

```fish
$ cargo run --release --features crawler --bin crawler -- --help

OPTIONS:
    -c, --crawl-interval <CRAWL_INTERVAL>
            The main crawling loop interval in seconds [default: 5]

    -h, --help
            Print help information

    -r, --rpc-addr <RPC_ADDR>
            If present, start an RPC server at the specified address

    -s, --seed-addrs <SEED_ADDRS>...
            The initial addresses to connect to

    -V, --version
            Print version information
```

`--seed-addrs` is the only required argument and needs at least one specified address for it to run.

## Metrics

The crawler collects some data for each node it visits and computes some metrics from that data. By default, it will only print and log these on exit (`Ctrl-C`) to a file called `crawler-log.txt`, unless the `--rpc-addr` argument is supplied, in which case these metrics will also be made available to RPC requests.

Fetching metrics from the RPC via `cURL` (piping through [`jq`](https://github.com/stedolan/jq) for prettier output):

```fish
$ curl --data-binary '{"jsonrpc": "2.0", "id":0, "method": "ge
tmetrics", "params": [] }' -H 'content-type: application/json'
 http://127.0.0.1:54321/ | jq .result
```

A sample of the data we collect and metrics we compute (obtained via RPC):

```json
{
  "num_known_nodes": 13654,
  "num_good_nodes": 2066,
  "num_known_connections": 1888893,
  "num_versions": 2019,
  "protocol_versions": {
    "170017": 10,
    "170018": 1958,
    "170016": 1,
    "170100": 50
  },
  "user_agents": {
    "/MagicBean:4.0.1/": 1,
    "/MagicBean:5.1.0-rc1/": 2,
    "/MagicBean:5.1.0/": 28,
    "/MagicBean:5.0.2/": 7,
    "/MagicBean:5.2.0/": 2,
    "/MagicBean:6.0.0/": 1957,
    "/MagicBean:5.1.1/": 1,
    "/Zebra:1.0.0-beta.10/": 1,
    "/MagicBean:6.0.0(bitcore)/": 1,
    "/MagicBean:5.0.0/": 19
  },
  "crawler_runtime": {
    "secs": 145,
    "nanos": 298240944
  },
  "density": 0.020101430617415265,
  "degree_centrality_delta": 1178,
  "avg_degree_centrality": 274
}
```

