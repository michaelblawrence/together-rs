# Together

A tool to run multiple commands in parallel selectively by an interactive prompt. Inspired by [concurrently](https://www.npmjs.com/package/concurrently).

[![Crates.io][crates-badge]][crates-url]
[![MIT licensed][mit-badge]][mit-url]
[![Build Status][actions-badge]][actions-url]

[crates-badge]: https://img.shields.io/crates/v/together-rs.svg
[crates-url]: https://crates.io/crates/together-rs
[mit-badge]: https://img.shields.io/badge/license-MIT-blue.svg
[mit-url]: https://github.com/michaelblawrence/together-rs/blob/master/LICENSE
[actions-badge]: https://github.com/michaelblawrence/together-rs/actions/workflows/rust.yml/badge.svg
[actions-url]: https://github.com/michaelblawrence/together-rs/actions

## Installation

<!-- You will need to have Rust installed to install `together` in the most straightforward way. You can install Rust by following the instructions on the [official website](https://www.rust-lang.org/tools/install). -->
The easiest way to install `together` is to use `cargo`. If you don't have `cargo` installed, you can install it by following the instructions on the [Rust official website](https://www.rust-lang.org/tools/install).

```sh
cargo install together-rs
```

Alternatively, `together` can be installed using the pre-built binaries for your platform. You can find the latest release on the [releases page](https://github.com/michaelblawrence/together-rs/releases). Download the binary for your platform and add it to your PATH.

```sh
# For example, on macOS
curl -L https://github.com/michaelblawrence/together-rs/releases/download/0.3.0/together-rs_0.3.0_x86_64-apple-darwin.zip -o together.zip
unzip together.zip
mv together /usr/local/bin
```

## Usage

### Getting Started
To start the interactive prompt:

```sh
together run -- "echo 'hello'" "echo 'world'"
```

This allows you to select which of the commands to run in parallel.

To run all commands in parallel:

```sh
together run --all -- "echo 'hello'" "echo 'world'"
```

### Example
In the example below we are using [yarn workspace](https://classic.yarnpkg.com/lang/en/docs/workspaces/) to run multiple package.json scripts, forwarding stdout/stderr to the terminal:

```sh
together run --raw -- \
 "yarn workspace server dev" \
 "yarn workspace client dev" \
 "yarn workspace event-processor dev" \
 "yarn workspace api-types watch"
```


Running the above command will start `together` and you will see the following interactive prompt:

![image](https://github.com/michaelblawrence/together-rs/assets/34494547/a788ba90-1c6c-4543-a29d-6d2d3fc17f44)

You can select which commands to run by pressing `spacebar` to toggle the selection. Press `enter` to start the selected commands.

`together` works well with monorepos, allowing you to run all of your required executables in the same terminal window.

Need to kill or restart a single command? Press `?` at any time to see options on managing commands (see below).


### Managing Commands
While the interactive prompt is running, you can manage the commands by pressing the following keys while `together` is running:

- `t`: Trigger another command to start
- `k`: Kill a running command
- `r`: Restart a running command
- `h` or `?`: Show the help message, which also lists all running commands, and more interactive options

### Configuration

Every time you run `together`, it saves the configuration to local disk.

You can use the `together load [yml_path]` option to specify a configuration file to use. Or use the following command to start `together` with the last saved configuration:

```sh
together rerun
```

## Contributing

If you're interested in contributing to the project, you can start by cloning the repository and building the project:

```sh
git clone https://github.com/michaelblawrence/together-rs.git
cd together
cargo build
```

Please follow standard Rust community guidelines and submit a PR on our repository.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
