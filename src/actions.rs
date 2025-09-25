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

use crate::args::{ArchCloneCli, AurCloneCli, BuildCli, GitCloneCli};
use crate::config::Config;

use pkg_manage_util::{chroot_build, git_utils};

use std::env;
use std::time::Duration;

use anyhow::{Context, Result};
use tracing::error;
use uuid::Uuid;

/// AUR Github mirror. used in case of an accident
const AUR_MIRROR_URL: &str = "https://github.com/archlinux/aur.git";

pub fn dump_config(config: &Config) -> Result<()> {
    let config_dump = config.dump_config()?;
    print!("{config_dump}");

    Ok(())
}

pub fn build_pkg(config: &Config, args: &BuildCli) -> Result<()> {
    let chroot_dir = config.chroot_dir();
    let build_paccachedir = config.build_paccachedir();
    let timeout = config.timeout();

    // if user provided config, overwrite config one
    let makepkgconf_path = if let Some(makepkg_conf) = &args.makepkg_conf {
        makepkg_conf.clone()
    } else {
        config.makepkgconf_path()
    };
    let pacmanconf_path = if let Some(pacman_conf) = &args.pacman_conf {
        pacman_conf.clone()
    } else {
        config.pacmanconf_path()
    };

    // setup temp chroot
    chroot_build::setup_chroot(
        chroot_dir.to_str().unwrap(),
        makepkgconf_path.to_str().unwrap(),
        pacmanconf_path.to_str().unwrap(),
        build_paccachedir.map(|p| p.to_str().unwrap().to_string()),
        None,
    )?;

    // lets use random uuid for the temp chroot name
    let chroot_folder = format!("build_{}", Uuid::new_v4());

    // if user didn't provide PKGBUILD path. use current dir
    let pkgbuild_path = if let Some(pkgbuild_path) = &args.pkgbuild_path {
        pkgbuild_path.clone()
    } else {
        env::current_dir().context("failed to get current dir")?.join("PKGBUILD")
    };

    let build_params = chroot_build::BuildParams {
        pkgbuild_path: pkgbuild_path.to_str().unwrap().to_string(),
        chroot_folder,
        chroot_parent: chroot_dir.to_str().unwrap().to_string(),
        makepkgconf_path: makepkgconf_path.to_str().unwrap().to_string(),
        makepkgconf_parent: makepkgconf_path.parent().unwrap().to_str().unwrap().to_string(),
        makechrootpkg_flag: String::new(),
        timeout: timeout.map(Duration::from_secs),
    };

    let result = chroot_build::build_package(build_params, None);

    // cleanup temp chroot
    chroot_build::clean_chroot_dir(chroot_dir.to_str().unwrap());

    println!("Build log:\n{}", result.build_log);
    if result.success {
        println!("Build successful!");
    } else {
        println!("Build failed!");
    }

    Ok(())
}

pub fn clone_arch_repo(config: &Config, args: &ArchCloneCli) -> Result<()> {
    let current_dir = env::current_dir().context("failed to get current dir")?;
    let dest_path = current_dir.join(&args.pkgbase);
    chroot_build::fetch_archpkgbuild(
        &args.pkgbase,
        args.version.as_deref().unwrap_or("main"),
        &dest_path,
        config.proxy_url(),
    )?;

    Ok(())
}

pub fn clone_aur_repo(config: &Config, args: &AurCloneCli) -> Result<()> {
    let current_dir = env::current_dir().context("failed to get current dir")?;
    let dest_path = current_dir.join(&args.pkgbase);

    if dest_path.exists() {
        anyhow::bail!("Destination path cannot be existing git repository");
    }

    let git_url = format!("https://aur.archlinux.org/{}.git", args.pkgbase);
    let res = git_utils::git_repo_clone(
        &git_url,
        args.depth,
        None,
        &dest_path,
        false,
        config.proxy_url(),
    );

    // fallback to AUR Github mirror
    if let Err(clone_err) = res {
        error!(
            "Failed to clone package {} from AUR with error '{clone_err}'! Falling back to Github \
             mirror",
            args.pkgbase
        );
    } else {
        return Ok(());
    }

    // fetch and clone just single remote
    git_utils::git_repo_clone(
        AUR_MIRROR_URL,
        None,
        Some(args.pkgbase.clone()),
        &dest_path,
        true,
        config.proxy_url(),
    )?;

    Ok(())
}

pub fn clone_git_repo(config: &Config, args: &GitCloneCli) -> Result<()> {
    git_utils::git_repo_clone(
        &args.git_url,
        args.depth,
        args.branch.clone(),
        &args.dest_path,
        args.single_branch,
        config.proxy_url(),
    )?;

    Ok(())
}
