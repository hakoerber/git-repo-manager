# Formatting & Style

## Code formatting

I'm allergic to discussions about formatting. I'd rather make the computer do it
for me.

For Rust, just use `cargo fmt`. For Python, use
[black](https://github.com/psf/black). I'd rather not spend any effort in
configuring the formatters (not possible for black anyway). For shell scripts,
use [`shfmt`](https://github.com/mvdan/sh).

To autoformat all code, use `just fmt`

## Style

Honestly, no idea about style. I'm still learning Rust, so I'm trying to find a
good style. Just try to keep it consistent when you add code.

## Linting

You can use `just lint` to run all lints.

### Rust

Clippy is the guard that prevents shitty code from getting into the code base.
When running `just check`, any clippy suggestions will make the command fail.
So make clippy happy! The easiest way:

* Commit your changes (so clippy can change safely).
* Run `cargo clippy --fix` to do the easy changes automatically.
* Run `cargo clippy` and take a look at the messages.

Until now, I had no need to override or silence any clippy suggestions.

### Shell

`shellcheck` lints all shell scripts. As they change very rarely, this is not
too important.

## Unsafe code

Any `unsafe` code is forbidden for now globally via `#![forbid(unsafe_code)]`.
I cannot think of any reason GRM may need `unsafe`. If it comes up, it needs to
be discussed.
