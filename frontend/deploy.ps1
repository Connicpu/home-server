$env:CARGO_TARGET_DIR="../target/trunk"

Try {
    trunk build --release

    scp dist/* "pi@pi.iot.connieh.com:home-server-www/"
}
Finally {
    $env:CARGO_TARGET_DIR=""
}
