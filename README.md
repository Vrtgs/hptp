# [HPTP] (High performance TCP proxy)

hptp is a high-performance TCP proxy designed to forward traffic to a specified host and ports with configurable runtime options.

## Features
- Supports both IPv4 and IPv6.
- Configurable logging levels.
- Choice between single-threaded and multi-threaded runtime.
- Fast and efficient, using splice sys calls on linux.

## Usage

### Command Line Arguments

- `--v4` (alias: `--ipv4`): Enable IPv4.
- `--v6` (alias: `--ipv6`): Enable IPv6.
- `--host <HOST>`: Specify the host to forward traffic to.
- `--ports <PORTS>`: Specify the ports to forward traffic to.
- `--runtime <Runtime type>` (alias: `--rt`): Specify runtime (default `single-threaded`).

by default if neither `--v4` or `--v6` are specified, `--v4` is enabled

### Example
`hptp --host example.com --ports [80,443] --log info --runtime multi-threaded`

this would route all IPv4 traffic on 0.0.0.0 on ports 80 and 443 to example.com:\<port>
## Configuration

### Runtime Types
- `single-threaded`: Runs the proxy on a single thread.
- `multi-threaded`: Runs the proxy on multiple threads.

### Ports Array

The ports array allows you to specify which ports the proxy should forward traffic to. You can define this array in several ways:

1. **Individual Ports**: You can list individual port numbers separated by commas.
    - Example: `[80, 443, 8080]`

2. **Inclusive Ranges**: You can specify a range of ports using the `..` syntax, which includes both the start and end values.
    - Example: `[80..90]` (This includes ports 80, 81, ..., 90)

3. **Exclusive Ranges**: You can specify a range of ports using the `..!=` syntax, which includes the start value but excludes the end value.
    - Example: `[80..!=90]` (This includes ports 80, 81, ..., 89)

The ports array is parsed from a string representation, and it supports a mix of individual ports and ranges. Here’s how it works:

- The string should start with `[` and end with `]`.
- Inside the brackets, ports and ranges are separated by commas.
- Each element can be an individual port, an inclusive range, or an exclusive range.

#### Examples
`[80, 443, 20..24, 2040..!=2080]` <br>
`[80..90, 443, 8080]` <br>

### Host

The `host` parameter specifies the destination host to which the proxy will forward traffic. This can be an IP address or a hostname. Here are some examples:

- **IPv4 Address**: `127.0.0.1`
- **IPv6 Address**: `::1`
- **Hostname**: `example.com`

The host parameter ensures that all traffic received by the proxy is directed to the specified host. This is useful for scenarios where you want to centralize traffic management or redirect traffic to a specific server.
