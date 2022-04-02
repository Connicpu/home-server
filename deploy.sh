#!/bin/bash

cargo build --release --target armv7-unknown-linux-gnueabihf

ssh pi@pi.iot.connieh.com "systemctl --user stop home-server.service"
scp target/armv7-unknown-linux-gnueabihf/release/home-server pi@pi.iot.connieh.com:
ssh pi@pi.iot.connieh.com "systemctl --user start home-server.service"
