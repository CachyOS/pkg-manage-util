// Copyright (C) 2025-2026 Vladislav Nepogodin
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

use crate::git_utils;

use std::path::Path;

use anyhow::Result;

fn convert_archrepo_url(orig_url: &str) -> String {
    // looks like archlinux's gitlab now always have converted URL
    if orig_url.contains('+') {
        let occurrences = orig_url.chars().filter(|x| *x == '+').count();

        // replace all occurrences of '+', and try again.
        // 1. if there is only one occurrence, replace it with '-'.
        // 2. if there are more than one occurrence, replace them with 'plus'.
        // arch gitlab config seems to convert '+' to 'plus' in urls.
        orig_url.replace('+', if occurrences > 1 { "plus" } else { "-" })
    } else {
        orig_url.to_owned()
    }
}

/// Clones a git repository, applying Arch Linux URL normalization.
///
/// # Errors
///
/// * The repository URL is invalid or unreachable.
/// * The `repo_path` is not accessible or not empty.
/// * The underlying git clone operation fails.
pub fn git_repo_clone<PathLike: AsRef<Path>>(
    repo_url: &str,
    repo_depth: Option<i32>,
    repo_branch: Option<&str>,
    repo_path: PathLike,
    single_branch: bool,
    proxy_url: Option<&str>,
) -> Result<()> {
    let repo_url_str = convert_archrepo_url(repo_url);
    git_utils::git_repo_clone(
        &repo_url_str,
        repo_depth,
        repo_branch,
        repo_path,
        single_branch,
        proxy_url,
    )
}

/// Clones a git repository and checks out a specific tag, applying Arch Linux URL normalization.
///
/// # Errors
///
/// * The clone operation fails.
/// * The specified tag does not exist.
pub fn git_repo_clone_tag<PathLike: AsRef<Path>>(
    repo_url: &str,
    repo_tag: &str,
    repo_path: PathLike,
    single_branch: bool,
    proxy_url: Option<&str>,
) -> Result<()> {
    let repo_url_str = convert_archrepo_url(repo_url);
    git_utils::git_repo_clone_tag(&repo_url_str, repo_tag, repo_path, single_branch, proxy_url)
}

/// Pulls changes for the current (or specified) branch from a remote.
///
/// # Errors
///
/// * The `repo_path` is not a valid git repository.
/// * The remote cannot be contacted.
/// * There are merge conflicts that cannot be resolved automatically.
pub fn git_repo_pull<PathLike: AsRef<Path>>(
    repo_path: PathLike,
    remote_name: Option<&str>,
    remote_branch: Option<&str>,
    proxy_url: Option<&str>,
) -> Result<()> {
    git_utils::git_repo_pull(repo_path, remote_name, remote_branch, proxy_url)
}

/// Pulls changes from the remote and checks out a specific tag.
///
/// # Errors
///
/// * The fetch or merge operations fail.
/// * The specified `remote_tag` cannot be checked out.
pub fn git_repo_pull_tag<PathLike: AsRef<Path>>(
    repo_path: PathLike,
    remote_name: Option<&str>,
    remote_tag: &str,
    proxy_url: Option<&str>,
) -> Result<()> {
    git_utils::git_repo_pull_tag(repo_path, remote_name, remote_tag, proxy_url)
}
