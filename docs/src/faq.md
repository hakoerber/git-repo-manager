# FAQ

## Why is the nightly toolchain required?

Building GRM currently requires nightly features due to the usage of
[`std::path::Path::is_symlink()`](https://doc.rust-lang.org/std/fs/struct.FileType.html#method.is_symlink).
See the [tracking issue](https://github.com/rust-lang/rust/issues/85748).

`is_symlink()` is actually available in rustc 1.57, so it will be on stable in
the near future. This would mean that GRM can be built using the stable toolchain!
