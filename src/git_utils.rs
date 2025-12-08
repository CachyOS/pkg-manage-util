// Copyright (C) 2023-2025 Vladislav Nepogodin
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

use anyhow::Result;
use git2::build::{CheckoutBuilder, RepoBuilder};
use git2::{FetchOptions, ProxyOptions, RemoteCallbacks, Repository};

#[cfg(debug_assertions)]
use {
    git2::Progress,
    std::cell::RefCell,
    std::io::{self, Write},
    std::path::PathBuf,
};

#[cfg(debug_assertions)]
struct State {
    progress: Option<Progress<'static>>,
    total: usize,
    current: usize,
    path: Option<PathBuf>,
    newline: bool,
}

#[cfg(debug_assertions)]
fn print(state: &mut State) {
    let stats = state.progress.as_ref().unwrap();
    let network_pct = (100 * stats.received_objects()) / stats.total_objects();
    let index_pct = (100 * stats.indexed_objects()) / stats.total_objects();
    let co_pct = if state.total > 0 { (100 * state.current) / state.total } else { 0 };
    let kbytes = stats.received_bytes() / 1024;
    if stats.received_objects() == stats.total_objects() {
        if !state.newline {
            println!();
            state.newline = true;
        }
        print!("Resolving deltas {}/{}\r", stats.indexed_deltas(), stats.total_deltas());
    } else {
        print!(
            "net {:3}% ({:4} kb, {:5}/{:5})  /  idx {:3}% ({:5}/{:5})  /  chk {:3}% ({:4}/{:4}) \
             {}\r",
            network_pct,
            kbytes,
            stats.received_objects(),
            stats.total_objects(),
            index_pct,
            stats.indexed_objects(),
            stats.total_objects(),
            co_pct,
            state.current,
            state.total,
            state.path.as_ref().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
        )
    }
    io::stdout().flush().unwrap();
}

fn do_fetch<'a>(
    repo: &'a git2::Repository,
    refs: &[&str],
    remote: &'a mut git2::Remote,
    proxy_url: Option<&str>,
) -> Result<git2::AnnotatedCommit<'a>, git2::Error> {
    let mut cb = git2::RemoteCallbacks::new();

    // Print out our transfer progress.
    #[cfg(debug_assertions)]
    cb.transfer_progress(|stats| {
        if stats.received_objects() == stats.total_objects() {
            print!("Resolving deltas {}/{}\r", stats.indexed_deltas(), stats.total_deltas());
        } else if stats.total_objects() > 0 {
            print!(
                "Received {}/{} objects ({}) in {} bytes\r",
                stats.received_objects(),
                stats.total_objects(),
                stats.indexed_objects(),
                stats.received_bytes()
            );
        }
        io::stdout().flush().unwrap();
        true
    });

    let mut fo = git2::FetchOptions::new();
    fo.remote_callbacks(cb);
    // Always fetch all tags.
    // Perform a download and also update tips
    fo.download_tags(git2::AutotagOption::All);

    let mut po = ProxyOptions::new();
    if let Some(proxy_url) = proxy_url {
        po.url(proxy_url);
    }
    fo.proxy_options(po);

    #[cfg(debug_assertions)]
    println!("Fetching {} for repo", remote.name().unwrap());

    remote.fetch(refs, Some(&mut fo), None)?;

    // If there are local objects (we got a thin pack), then tell the user
    // how many objects we saved from having to cross the network.
    #[cfg(debug_assertions)]
    {
        let stats = remote.stats();
        if stats.local_objects() > 0 {
            println!(
                "\rReceived {}/{} objects in {} bytes (used {} local objects)",
                stats.indexed_objects(),
                stats.total_objects(),
                stats.received_bytes(),
                stats.local_objects()
            );
        } else {
            println!(
                "\rReceived {}/{} objects in {} bytes",
                stats.indexed_objects(),
                stats.total_objects(),
                stats.received_bytes()
            );
        }
    }

    let fetch_head = repo.find_reference("FETCH_HEAD")?;
    repo.reference_to_annotated_commit(&fetch_head)
}

fn fast_forward(
    repo: &Repository,
    lb: &mut git2::Reference,
    rc: &git2::AnnotatedCommit,
) -> Result<(), git2::Error> {
    let name = match lb.name() {
        Some(s) => s.to_string(),
        None => String::from_utf8_lossy(lb.name_bytes()).to_string(),
    };
    let msg = format!("Fast-Forward: Setting {} to id: {}", name, rc.id());
    println!("{msg}");
    lb.set_target(rc.id(), &msg)?;
    repo.set_head(&name)?;
    repo.checkout_head(Some(git2::build::CheckoutBuilder::default()
            // For some reason the force is required to make the working directory actually get updated
            // I suspect we should be adding some logic to handle dirty working directory states
            // but this is just an example so maybe not.
            .force()))?;
    Ok(())
}

fn normal_merge(
    repo: &Repository,
    local: &git2::AnnotatedCommit,
    remote: &git2::AnnotatedCommit,
) -> Result<(), git2::Error> {
    let local_tree = repo.find_commit(local.id())?.tree()?;
    let remote_tree = repo.find_commit(remote.id())?.tree()?;
    let ancestor = repo.find_commit(repo.merge_base(local.id(), remote.id())?)?.tree()?;
    let mut idx = repo.merge_trees(&ancestor, &local_tree, &remote_tree, None)?;

    if idx.has_conflicts() {
        println!("Merge conflicts detected...");
        repo.checkout_index(Some(&mut idx), None)?;
        return Ok(());
    }
    let result_tree = repo.find_tree(idx.write_tree_to(repo)?)?;
    // now create the merge commit
    let msg = format!("Merge: {} into {}", remote.id(), local.id());
    let sig = repo.signature()?;
    let local_commit = repo.find_commit(local.id())?;
    let remote_commit = repo.find_commit(remote.id())?;
    // Do our merge commit and set current branch head to that commit.
    let _merge_commit = repo
        .commit(Some("HEAD"), &sig, &sig, &msg, &result_tree, &[&local_commit, &remote_commit])?;
    // Set working tree to match head.
    repo.checkout_head(None)?;
    Ok(())
}

fn do_merge<'a>(
    repo: &'a Repository,
    remote_branch: &str,
    fetch_commit: git2::AnnotatedCommit<'a>,
) -> Result<(), git2::Error> {
    // 1. do a merge analysis
    let analysis = repo.merge_analysis(&[&fetch_commit])?;

    // 2. Do the appropriate merge
    if analysis.0.is_fast_forward() {
        #[cfg(debug_assertions)]
        println!("Doing a fast forward");

        // do a fast forward
        let refname = format!("refs/heads/{remote_branch}");
        match repo.find_reference(&refname) {
            Ok(mut r) => {
                fast_forward(repo, &mut r, &fetch_commit)?;
            },
            Err(_) => {
                // The branch doesn't exist so just set the reference to the
                // commit directly. Usually this is because you are pulling
                // into an empty repository.
                repo.reference(
                    &refname,
                    fetch_commit.id(),
                    true,
                    &format!("Setting {} to {}", remote_branch, fetch_commit.id()),
                )?;
                repo.set_head(&refname)?;
                repo.checkout_head(Some(
                    git2::build::CheckoutBuilder::default()
                        .allow_conflicts(true)
                        .conflict_style_merge(true)
                        .force(),
                ))?;
            },
        };
    } else if analysis.0.is_normal() {
        // do a normal merge
        let head_commit = repo.reference_to_annotated_commit(&repo.head()?)?;
        normal_merge(repo, &head_commit, &fetch_commit)?;
    } else {
        #[cfg(debug_assertions)]
        println!("Nothing to do...");
    }
    Ok(())
}

pub fn git_repo_clone<PathLike: AsRef<Path>>(
    repo_url: &str,
    repo_depth: Option<i32>,
    repo_branch: Option<String>,
    repo_path: PathLike,
    single_branch: bool,
    proxy_url: Option<String>,
) -> Result<()> {
    #[cfg(debug_assertions)]
    let state =
        RefCell::new(State { progress: None, total: 0, current: 0, path: None, newline: false });

    let mut cb = RemoteCallbacks::new();

    #[cfg(debug_assertions)]
    cb.transfer_progress(|stats| {
        let mut state = state.borrow_mut();
        state.progress = Some(stats.to_owned());
        print(&mut state);
        true
    });

    let mut co = CheckoutBuilder::new();

    #[cfg(debug_assertions)]
    co.progress(|path, cur, total| {
        let mut state = state.borrow_mut();
        state.path = path.map(|p| p.to_path_buf());
        state.current = cur;
        state.total = total;
        print(&mut state);
    });

    let mut fo = FetchOptions::new();
    fo.remote_callbacks(cb);
    // Always fetch all tags.
    // Perform a download and also update tips
    fo.download_tags(git2::AutotagOption::All);

    let mut po = ProxyOptions::new();
    if let Some(proxy_url) = proxy_url {
        po.url(&proxy_url);
    }
    fo.proxy_options(po);

    if let Some(repo_depth) = repo_depth {
        fo.depth(repo_depth);
    }

    let mut repo_builder = RepoBuilder::new();
    repo_builder.fetch_options(fo).with_checkout(co);

    // TODO(vnepogodin): add proxy on the 429 code
    if let Some(repo_branch) = repo_branch {
        repo_builder.branch(&repo_branch);

        if single_branch {
            let refspec = format!("+refs/heads/{0:}:refs/remotes/origin/{0:}", &repo_branch);
            repo_builder
                .remote_create(move |repo, name, url| repo.remote_with_fetch(name, url, &refspec));
        }
    }
    repo_builder.clone(repo_url, repo_path.as_ref())?;
    // if let Err(clone_error) = repo_builder.clone(&repo_url, Path::new(&repo_path)) {
    //    println!("clone {:?}", clone_error.class());
    //}
    Ok(())
}

pub fn git_repo_clone_tag<PathLike: AsRef<Path>>(
    repo_url: &str,
    repo_tag: &str,
    repo_path: PathLike,
    single_branch: bool,
    proxy_url: Option<String>,
) -> Result<()> {
    git_repo_clone(repo_url, None, None, repo_path.as_ref(), single_branch, proxy_url)?;
    git_repo_checkout(repo_path, repo_tag)?;

    Ok(())
}

pub fn git_repo_pull<PathLike: AsRef<Path>>(
    repo_path: PathLike,
    remote_name: Option<String>,
    remote_branch: Option<String>,
    proxy_url: Option<String>,
) -> Result<()> {
    let remote_name = remote_name.as_ref().map(|s| &s[..]).unwrap_or("origin");

    let repo = Repository::open(repo_path.as_ref())?;
    let mut remote = repo.find_remote(remote_name)?;
    let mut was_none_branch = true;
    let remote_branch = if let Some(remote_branch) = remote_branch {
        was_none_branch = false;
        remote_branch.clone()
    } else {
        let head = repo.head()?;
        head.shorthand().unwrap().to_owned()
    };

    let fetch_commit = do_fetch(&repo, &[&remote_branch], &mut remote, proxy_url.as_deref())?;
    do_merge(&repo, &remote_branch, fetch_commit)?;

    // NOTE: Do we need it here??
    if was_none_branch {
        git_repo_checkout(repo_path, &remote_branch)?;
    }

    Ok(())
}

pub fn git_repo_pull_tag<PathLike: AsRef<Path>>(
    repo_path: PathLike,
    remote_name: Option<String>,
    remote_tag: &str,
    proxy_url: Option<String>,
) -> Result<()> {
    let remote_name = remote_name.as_ref().map(|s| &s[..]).unwrap_or("origin");

    let repo = Repository::open(repo_path.as_ref())?;
    let mut remote = repo.find_remote(remote_name)?;

    let fetch_commit = do_fetch(&repo, &[], &mut remote, proxy_url.as_deref())?;

    let remote_branch = {
        let head = repo.head()?;
        head.shorthand().unwrap().to_owned()
    };
    do_merge(&repo, &remote_branch, fetch_commit)?;

    git_repo_checkout(repo_path, remote_tag)?;

    Ok(())
}

pub fn git_repo_checkout<PathLike: AsRef<Path>>(repo_path: PathLike, ref_name: &str) -> Result<()> {
    let repo = Repository::open(repo_path)?;
    let refer = repo.resolve_reference_from_short_name(ref_name)?;
    let obj = refer.peel(git2::ObjectType::Any)?;
    repo.set_head_detached(obj.id())?;

    Ok(())
}
