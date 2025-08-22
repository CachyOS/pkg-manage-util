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
        orig_url.replace("+", if occurrences > 1 { "plus" } else { "-" })
    } else {
        orig_url.to_owned()
    }
}

pub fn git_repo_clone<PathLike: AsRef<Path>>(
    repo_url: &str,
    repo_depth: Option<i32>,
    repo_branch: Option<String>,
    repo_path: PathLike,
    proxy_url: Option<String>,
) -> Result<()> {
    let repo_url_str = convert_archrepo_url(repo_url);
    git_utils::git_repo_clone(&repo_url_str, repo_depth, repo_branch.clone(), repo_path, proxy_url)
}

pub fn git_repo_clone_tag<PathLike: AsRef<Path>>(
    repo_url: &str,
    repo_tag: &str,
    repo_path: PathLike,
    proxy_url: Option<String>,
) -> Result<()> {
    let repo_url_str = convert_archrepo_url(repo_url);
    git_utils::git_repo_clone_tag(&repo_url_str, repo_tag, repo_path, proxy_url)
}

pub fn git_repo_pull<PathLike: AsRef<Path>>(
    repo_path: PathLike,
    remote_name: Option<String>,
    remote_branch: Option<String>,
    proxy_url: Option<String>,
) -> Result<()> {
    git_utils::git_repo_pull(repo_path, remote_name.clone(), remote_branch.clone(), proxy_url)
}

pub fn git_repo_pull_tag<PathLike: AsRef<Path>>(
    repo_path: PathLike,
    remote_name: Option<String>,
    remote_tag: &str,
    proxy_url: Option<String>,
) -> Result<()> {
    git_utils::git_repo_pull_tag(repo_path, remote_name.clone(), remote_tag, proxy_url)
}
