#!/bin/bash
#
# A quick and a simple script to be used for any debugging or testing.

NUM_NODES=$1
ADDR=$2
ACTION=$3

## Possible actions:
# SendGetAddrAndForeverSleep / AdvancedSnForS001 / QuickConnectAndThenCleanDisconnect /
# QuickConnectWithImproperDisconnect / ConstantlyAskForRandomBlocks

TRACE_LOG=info
BIN=../target/debug/synth_node_bin

for i in $(seq 1 $NUM_NODES);
do
    RUST_LOG=$TRACE_LOG $BIN -t -s -n $ADDR  -a $ACTION &
done
