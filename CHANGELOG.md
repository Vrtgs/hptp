# Changelog

All notable changes to this project will be documented in this file.

This project adheres to [Semantic Versioning](https://semver.org).

## [0.0.13] - 2024-09-03
- update dependencies
- remove `-p` short form

## [0.0.12] - 2024-09-03
- only run multi-threaded runtime when there is more than one thread available
- stop using custom allocator
- ability to add to start up and run as a daemon
- new subcommands (run, set-up-daemon, remove-daemon)


## [0.0.11] - 2024-08-05
- only run multi-threaded runtime when there is more than one thread available
- stop using custom allocator

## [0.0.10] - 2024-08-05
- update README.md
- update dependencies

## [0.0.9] - 2024-08-05
- update cli arguments

## [0.0.8] - 2024-08-05
- crates.io release

## [0.0.7] - 2024-07-18
- better performance
- if ip no flags are set, assume ipv4

## [0.0.6] - 2024-07-16
- added macOS and windows builds
- experimental memory alloc, fixed tests with miri

## [0.0.5] - 2024-07-16
- better performance with splice syscall on available linux targets
- update dependencies


## [0.0.4] - 2024-07-16
- performance improvements
- switch to using the `tracing` crate for logging

## [0.0.3] - 2024-07-08
- supports different logging levels at runtime
- new panic hook to log errors properly
- move to new cli module with plans on extending to gui, soon™

## [0.0.2] - 2024-07-06
- debug logging via the logging crate

## [0.0.1] - 2024-06-18
- Initial release