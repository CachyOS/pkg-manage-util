// Copyright (C) 2025-2026 Vladislav Nepogodin
//
// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version;
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License along
// with this program; if not, write to the Free Software Foundation, Inc.,
// 51 Franklin Street, Fifth Floor, Boston, MA 02110-1301 USA.

use crate::git_utils;

use std::path::Path;

use anyhow::Result;
use tracing::error;

/// AUR Github mirror. used in case of an accident
const AUR_MIRROR_URL: &str = "https://github.com/archlinux/aur.git";

/// Clones an AUR package repository.
///
/// This function attempts to clone the package source from the official Arch User Repository (AUR).
/// If the primary clone fails (e.g. package not found or connection refused), it falls back to
/// a GitHub mirror.
///
/// # Errors
///
/// * The `dest_path` already exists on the filesystem.
/// * The primary clone attempt fails **and** the fallback clone attempt also fails.
/// * The underlying git operations encounter network or I/O errors.
pub fn clone_repo<PathLike: AsRef<Path>>(
    pkgbase: &str,
    dest_path: PathLike,
    repo_depth: Option<i32>,
    proxy_url: Option<&str>,
) -> Result<()> {
    let dest_path = dest_path.as_ref();
    if dest_path.exists() {
        anyhow::bail!("Destination path cannot be existing git repository");
    }

    let git_url = format!("https://aur.archlinux.org/{pkgbase}.git");
    let res = git_utils::git_repo_clone(&git_url, repo_depth, None, dest_path, false, proxy_url);

    // fallback to AUR Github mirror
    if let Err(clone_err) = res {
        error!(
            "Failed to clone package {pkgbase} from AUR with error '{clone_err}'! Falling back to \
             Github mirror",
        );
    } else {
        return Ok(());
    }

    // fetch and clone just single remote
    git_utils::git_repo_clone(AUR_MIRROR_URL, None, Some(pkgbase), dest_path, true, proxy_url)?;

    Ok(())
}
