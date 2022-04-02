$env:CARGO_TARGET_DIR="../target/trunk"

Try {
    trunk serve --dist "../target/trunk/debug-dist"
}
Finally {
    $env:CARGO_TARGET_DIR=""
}
