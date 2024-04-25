# Maintainer: Hannes Körber <hannes@hkoerber.de>
pkgname='grm-git'
pkgver=0.7.15.r6.gea7299a
pkgrel=1
pkgdesc='Manage git repos, worktrees and integrate with GitHub and GitLab'
arch=('x86_64')
url='https://github.com/hakoerber/git-repo-manager'
license=('GPL-3.0-only')
depends=('glibc' 'gcc-libs' 'libgit2' 'curl' 'openssl')
makedepends=('cargo' 'git')
provides=('grm')
conflicts=('grm')
options=("!buildflags")
# options=()
source=("${pkgname}::git+https://github.com/hakoerber/git-repo-manager#branch=develop")
sha256sums=('SKIP')

pkgver() {
  cd "${pkgname}"
  git describe --long --tags --abbrev=7 | sed 's/^v//;s/\([^-]*-g\)/r\1/;s/-/./g'
}

prepare() {
  cd "${pkgname}"
  export RUSTUP_TOOLCHAIN=stable
  cargo fetch --locked --target "$(rustc -vV | sed -n 's/host: //p')"
}

build() {
  cd "${pkgname}"
  export RUSTUP_TOOLCHAIN=stable
  export CARGO_TARGET_DIR=target
  export GRM_RELEASE_VERSION="${pkgver}"
  # https://github.com/alexcrichton/ssh2-rs/issues/171#issue-580432290
  # export LIBSSH2_SYS_USE_PKG_CONFIG=
  cargo build --frozen --release
}

check() {
  cd "${pkgname}"
  export RUSTUP_TOOLCHAIN=stable
  cargo test --frozen
}

package() {
  cd "${pkgname}"
  install -Dm0755 -t "$pkgdir/usr/bin/" "target/release/${pkgname/-git}"
}
