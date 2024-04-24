#!/bin/sh

SCRIPTDIR=$(dirname $0)

$SCRIPTDIR/toolbox.sh run ~/.cargo/bin/rust-analyzer "$@"
