// Copyright (C) 2025 Vladislav Nepogodin
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

pub fn clone_repo<PathLike: AsRef<Path>>(
    pkgbase: &str,
    dest_path: PathLike,
    repo_depth: Option<i32>,
    proxy_url: Option<String>,
) -> Result<()> {
    let dest_path = dest_path.as_ref();
    if dest_path.exists() {
        anyhow::bail!("Destination path cannot be existing git repository");
    }

    let git_url = format!("https://aur.archlinux.org/{pkgbase}.git");
    let res =
        git_utils::git_repo_clone(&git_url, repo_depth, None, dest_path, false, proxy_url.clone());

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
    git_utils::git_repo_clone(
        AUR_MIRROR_URL,
        None,
        Some(pkgbase.into()),
        dest_path,
        true,
        proxy_url.clone(),
    )?;

    Ok(())
}
