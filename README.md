# Together

A tool to run multiple commands in parallel selectively by an interactive prompt. Inspired by [concurrently](https://www.npmjs.com/package/concurrently).

## Installation

You will need to have Rust installed to build the project. You can install Rust by following the instructions on the [official website](https://www.rust-lang.org/tools/install).

```sh
cargo install together
```

## Usage

### Getting Started
To start the interactive prompt:

```sh
together run "echo 'hello'" "echo 'world'"
```

This allows you to select which of the commands to run in parallel.

To run all commands in parallel:

```sh
together run --all "echo 'hello'" "echo 'world'"
```

### Managing Commands
While the interactive prompt is running, you can manage the commands by pressing the following keys while together is running:

- `t`: Trigger another command to start
- `k`: Kill a running command
- `r`: Restart a running command
- `h` or `?`: Show the help message, which also lists all running commands, and more interactive options

### Configuration

Every time you run together, it saves the configuration to local disk.

You can use the `together load [toml_path]` option to specify a configuration file to use. Or use the following command to start together with the last saved configuration:

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
