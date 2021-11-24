# Quickstart

## Installation

Building GRM currently requires the nightly Rust toolchain. The easiest way
is using [`rustup`](https://rustup.rs/). Make sure that rustup is properly installed.

Make sure that the nightly toolchain is installed:

```
$ rustup toolchain install nightly
```

```bash
$ cargo +nightly install --git https://github.com/hakoerber/git-repo-manager.git --branch master
```

If you're brave, you can also run the development build:

```bash
$ cargo +nightly install --git https://github.com/hakoerber/git-repo-manager.git --branch develop
```
