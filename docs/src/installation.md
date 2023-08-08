# Installation

## Installation

Building GRM requires the Rust toolchain to be installed. The easiest way is
using [`rustup`](https://rustup.rs/). Make sure that rustup is properly
installed.

Make sure that the stable toolchain is installed:

```
$ rustup toolchain install stable
```

Then, install the build dependencies:

| Distribution  | Command                                                                        |
| ------------- | ------------------------------------------------------------------------------ |
| Arch Linux    | `pacman -S --needed gcc openssl pkg-config`                                    |
| Ubuntu/Debian | `apt-get install --no-install-recommends pkg-config gcc libssl-dev zlib1g-dev` |

Then, it's a simple command to install the latest stable version:

```bash
$ cargo install git-repo-manager
```

If you're brave, you can also run the development build:

```bash
$ cargo install --git https://github.com/hakoerber/git-repo-manager.git --branch develop
```

## Static build

Note that by default, you will get a  dynamically linked executable.
Alternatively, you can also build a statically linked binary. For this, you will
need `musl` and a few other build dependencies installed installed:

| Distribution  | Command                                                                     |
| ------------- | --------------------------------------------------------------------------- |
| Arch Linux    | `pacman -S --needed gcc musl perl make`                                     |
| Ubuntu/Debian | `apt-get install --no-install-recommends gcc musl-tools libc-dev perl make` |

(`perl` and `make` are required for the OpenSSL build script)

The, add the musl target via `rustup`:

```
$ rustup target add x86_64-unknown-linux-musl
```

Then, use a modified build command to get a statically linked binary:

```
$ cargo install git-repo-manager --target x86_64-unknown-linux-musl --features=static-build
```
