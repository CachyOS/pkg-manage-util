// Copyright (C) 2025 Vladislav Nepogodin
//
// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License along
// with this program; if not, write to the Free Software Foundation, Inc.,
// 51 Franklin Street, Fifth Floor, Boston, MA 02110-1301 USA.

use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result};

/// Creates a detached GPG signature for the specified file.
///
/// # Errors
///
/// * The `gpg` binary cannot be found or executed.
/// * The `gpg` command returns a failure exit code (indicating signing failed).
///
/// # Panics
///
/// If the provided `filepath` contains non-UTF-8 characters.
pub fn create_detached_signature<PathLike: AsRef<Path>>(
    filepath: PathLike,
    sign_key: Option<String>,
) -> Result<()> {
    // construct args for gpg
    let mut gpg_args: Vec<&str> = vec!["--batch", "--detach-sign"];

    if let Some(sign_key) = &sign_key {
        gpg_args.extend_from_slice(&["-u", sign_key]);
    }
    gpg_args.push(filepath.as_ref().to_str().unwrap());

    let cmd = Command::new("/sbin/gpg")
        .args(&gpg_args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("failed to spawn gpg")?;

    if !cmd.success() {
        anyhow::bail!("failed to create detached signature");
    }

    Ok(())
}

/// Verifies the GPG signature for a specific file.
///
/// Assumes the signature file is located at `filepath + ".sig"`.
///
/// # Errors
///
/// * The `gpg` binary cannot be found or executed.
/// * The signature verification fails (exit code 1 or 2).
/// * The `gpg` process terminates unexpectedly.
///
/// # Panics
///
/// If the provided `filepath` contains non-UTF-8 characters.
pub fn verify_gpg_signature<PathLike: AsRef<Path>>(filepath: PathLike, name: &str) -> Result<()> {
    let filepath = filepath.as_ref().to_str().unwrap();
    let gpg_sign = format!("{filepath}.sig");
    let output = Command::new("/sbin/gpg")
        .args(["--verify", &gpg_sign])
        .output()
        .context("failed to spawn gpg")?;

    if let Some(exit_code) = output.status.code() {
        if exit_code == 0 {
            return Ok(());
        } else if exit_code == 1 || exit_code == 2 {
            anyhow::bail!("failed to verify gpg signature");
        }
    }

    let stderr = String::from_utf8_lossy(&output.stdout);
    anyhow::bail!("[{name}] signature check failed: {stderr}");
}

/// Imports a list of PGP keys from the Ubuntu keyserver.
///
/// # Errors
///
/// * The `gpg` binary cannot be found or executed.
/// * The key import process fails (non-zero exit code), potentially due to network issues or
///   invalid keys.
pub fn import_pgp_keys(pgpkeys: &[String]) -> Result<()> {
    if pgpkeys.is_empty() {
        return Ok(());
    }

    let mut args =
        vec!["--keyserver".to_owned(), "keyserver.ubuntu.com".to_owned(), "--recv-keys".to_owned()];
    args.extend_from_slice(pgpkeys);

    let cmd = Command::new("/sbin/gpg")
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("failed to spawn gpg")?;

    if !cmd.success() {
        anyhow::bail!("failed to import gpg keys");
    }

    Ok(())
}
