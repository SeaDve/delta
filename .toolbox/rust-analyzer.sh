#!/bin/sh

/app/bin/host-spawn -no-pty toolbox run ~/.cargo/bin/rust-analyzer "$@"
