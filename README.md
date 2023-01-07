# scrcap

A screenshot tool for Sway written in Rust.

## Build

scrcap is a Cargo based. Just execute
```sh
cargo build
```
or for a release
```
cargo build --release
```
After building the tool, it can be executed with Cargo directly
```sh
cargo run
```
or for a release
```sh
cargo run --release
```

## Usage

### Whole screen
Take a screenshot of the whole screen
```sh
scrcap --output-name DP-1
```
If no output name gets specified, then the first detected output will be used.

### Active window
To take a screenshot of the active window invoke `scrcap` like
```sh
scrcap --active
```

### Only a region
To take a screenshot of only a region the tool `slurp` and `xargs` needs to be installed.
```sh
slurp -f '--x=%x --y=%y --width=%w --height=%h' | xargs scrcap
```

### Enable logging
Select the logging level with the environment variable `RUST_LOG`.
```sh
RUST_LOG=DEBUG wayshot
```
The log level can be one of `DEBUG`, `INFO`, `WARN`, `ERROR`.

## Credits
[Wayshot](https://github.com/waycrate/wayshot)
