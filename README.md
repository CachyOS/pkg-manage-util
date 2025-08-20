# pkg-manage-util

A command-line utility for managing pacman package builds for Arch Linux. This tool allows for cloning packages from the Arch Linux repositories and other git repositories, and building them in a clean chroot environment.

## Features

-   Clone packages from Arch Linux repositories.
-   Clone any git repository.
-   Build packages in a clean chroot.
-   Dump the default configuration.

## Installation

Install from source using cargo:

```bash
git clone https://github.com/cachyos/pkg-manage-util.git
cd pkg-manage-util
cargo install --path .
```

## Usage

### Clone a package from Arch Linux repositories

```bash
pkg-manage-util clone-arch-repo <pkgbase> [version]
```

-   `<pkgbase>`: The name of the package to clone.
-   `[version]`: The specific version of the package to clone (optional).

### Clone a git repository

```bash
pkg-manage-util clone-git-repo <git_url> <dest_path> [--depth <depth>] [-b <branch>]
```

-   `<git_url>`: The URL of the git repository to clone.
-   `<dest_path>`: The destination path to clone the repository to.
-   `--depth <depth>`: The depth of the repository to clone (optional).
-   `-b <branch>`: The branch to clone (optional).

### Build a package

```bash
pkg-manage-util build [pkgbuild_path]
```

-   `[pkgbuild_path]`: The path to the PKGBUILD file (optional). If not provided, it will search for a PKGBUILD in the current directory.

### Dump the default configuration

```bash
pkg-manage-util dump-config
```

This will print the default configuration to stdout. You can redirect this to a file to create a custom configuration file.

## License

This project is licensed under the GPL-3.0-or-later. See the [LICENSE](LICENSE) file for details.
