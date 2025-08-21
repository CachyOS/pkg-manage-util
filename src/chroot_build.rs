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

use crate::{archgit_utils, subproc};

use std::path::Path;
use std::{env, fs, path};

use anyhow::{Context, Result};
use tokio::time::Duration;
use tracing::{debug, error};

#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BuildResult {
    pub success: bool,
    pub build_log: String,
}

/// @brief Struct to encapsulate parameters for chroot operations.
///
/// This structure bundles together common parameters needed when working with chroot environments,
/// such as paths to configuration files and the chroot root directory itself. It is used to
/// simplify function signatures and improve code readability when dealing with chroot-related
/// operations.
struct ChrootParams {
    /// @brief The path to the root directory of the chroot environment.
    ///
    /// This path specifies the location where the chroot environment is or will be created.
    /// For example, if `chroot_root_path` is "/path/to/chroot/root", then the chroot environment
    /// will be rooted at "/path/to/chroot/root". This should typically be an absolute path.
    chroot_root_path: String,
    /// @brief The path to the `makepkg.conf` file to be used within the chroot.
    ///
    /// This path specifies the location of the `makepkg.conf` file that will be copied into the
    /// chroot environment and used by `makepkg` during the build process. This should be a
    /// path to the `makepkg.conf` file on the host system that will be copied into the chroot.
    makepkgconf_path: String,
    /// @brief The path to the `pacman.conf` file to be used within the chroot.
    ///
    /// This path specifies the location of the `pacman.conf` file that will be copied into the
    /// chroot environment and used by `pacman` for package management within the chroot. This
    /// should be a path to the `pacman.conf` file on the host system that will be copied into
    /// the chroot.
    pacmanconf_path: String,
    /// @brief An optional path to a custom pacman cache directory to be used within the chroot.
    ///
    /// If provided, this path specifies a directory on the host system that will be used as a
    /// package cache within the chroot environment. This allows sharing a package cache
    /// between the host and the chroot, or between multiple chroot environments, potentially
    /// speeding up package installation and building. If not provided, pacman will use its
    /// default cache directory within the chroot.
    build_paccachedir: Option<String>,
    /// TODO: add documentation
    packages: Option<Vec<String>>,
}

pub struct BuildParams {
    /// The path to the PKGBUILD file.
    pub pkgbuild_path: String,
    /// The path to the chroot folder.
    pub chroot_folder: String,
    /// The path to the parent directory of the chroot folder (outside the chroot).
    pub chroot_parent: String,
    /// The path to the makepkg.conf file within the chroot.
    pub makepkgconf_path: String,
    /// The path to the parent directory of the makepkg.conf file (outside the chroot).
    pub makepkgconf_parent: String,
    /// An optional flag to use the custom pacman cache directory in makechrootpkg.
    pub makechrootpkg_flag: String,
    /// TODO: add documentation
    pub timeout: Option<Duration>,
}

fn construct_mkarchroot_args(chroot_params: &ChrootParams) -> Vec<String> {
    let mut args: Vec<String> = vec![];
    if let Some(build_paccachedir) = &chroot_params.build_paccachedir {
        args.extend_from_slice(&["-c".into(), build_paccachedir.clone()]);
    }

    args.extend_from_slice(&[
        "-C".into(),
        chroot_params.pacmanconf_path.clone(),
        "-M".into(),
        chroot_params.makepkgconf_path.clone(),
        chroot_params.chroot_root_path.clone(),
        "base-devel".into(),
    ]);
    // append user requested packages
    if let Some(packages) = &chroot_params.packages {
        args.extend_from_slice(packages);
    }

    args
}

fn construct_nspawn_args(chroot_params: &ChrootParams) -> Vec<String> {
    let mut args: Vec<String> = vec![];
    if let Some(build_paccachedir) = &chroot_params.build_paccachedir {
        args.extend_from_slice(&["-c".into(), build_paccachedir.clone()]);
    }

    args.extend_from_slice(&[
        "-C".into(),
        chroot_params.pacmanconf_path.clone(),
        chroot_params.chroot_root_path.clone(),
        "pacman".into(),
        "-Syuu".into(),
        "--noconfirm".into(),
    ]);
    args
}

fn construct_makechrootpkg_args(build_params: &BuildParams) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "-c".into(),
        "-D".into(),
        build_params.makepkgconf_parent.clone(),
        "-l".into(),
        build_params.chroot_folder.clone(),
        "-r".into(),
        build_params.chroot_parent.clone(),
    ];
    if !build_params.makechrootpkg_flag.is_empty() {
        let makechrootpkg_flag =
            build_params.makechrootpkg_flag.strip_prefix("-P ").unwrap().to_owned();
        args.extend_from_slice(&["-P".into(), makechrootpkg_flag]);
    }
    args.push("--".into());
    let makepkg_args: Vec<String> = vec![
        "--config".into(),
        build_params.makepkgconf_path.clone(),
        "-m".into(),
        "--noprogressbar".into(),
        "--syncdeps".into(),
        "--noconfirm".into(),
        "--holdver".into(),
        "--skipinteg".into(),
        "--skippgpcheck".into(),
        "--clean".into(),
        "--cleanbuild".into(),
    ];
    args.extend(makepkg_args);
    args
}

fn construct_buildpkgcmd_args(build_params: &BuildParams) -> Vec<String> {
    // TODO(vnepogodin): refactor that
    let mut args: Vec<String> = vec![
        "--preserve-env=SOURCE_DATE_EPOCH BUILDTOOL BUILDTOOLVER GNUPGHOME SRCDEST SRCPKGDEST \
         PKGDEST LOGDEST MAKEFLAGS PACKAGER"
            .into(),
        "--".into(),
        "/sbin/makechrootpkg".into(),
    ];
    let makechrootpkg_args = construct_makechrootpkg_args(build_params);
    args.extend(makechrootpkg_args);
    args
}

/// @brief Builds a package within a chroot environment.
///
/// This function executes `makechrootpkg` to build a package specified by the given PKGBUILD path
/// within a chroot environment. It utilizes `sudo` to run the build command with appropriate
/// environment variables and arguments.
pub fn build_package(build_params: BuildParams) -> BuildResult {
    let args = construct_buildpkgcmd_args(&build_params);
    debug!("running build := '{args:?}'");

    // Run build
    let pkgbuild_parent =
        Path::new(&build_params.pkgbuild_path).parent().unwrap_or(Path::new("/")).to_str().unwrap();
    debug!("Running in '{pkgbuild_parent}'...");

    let mut log: Vec<u8> = vec![];
    let res =
        subproc::exec_proc("sudo", &args, pkgbuild_parent, &mut log, build_params.timeout, None);

    let build_log = String::from_utf8_lossy(&log).to_string();
    if let Err(err) = &res {
        error!("Failed to run build with error: {err:?}");
    }
    let success = res.unwrap_or_default();

    BuildResult { success, build_log }
}

/// @brief Sets up a chroot environment for building packages.
///
/// This function initializes or updates a chroot environment located within `chroot_parent`.
/// If the chroot directory (named "root" inside `chroot_parent`) does not exist, it will be created
/// using `mkarchroot`. This involves installing a minimal Arch Linux base system along with
/// essential build tools like `base-devel`. The `pacman.conf` and
/// `makepkg.conf` files from the host system, specified by `pacmanconf_path` and `makepkgconf_path`
/// respectively, are copied into the newly created chroot to configure package management and
/// package building within the chroot.
///
/// If the chroot directory already exists, this function updates the chroot environment by running
/// `pacman -Syuu` inside the chroot using `arch-nspawn`. This ensures that the packages within the
/// chroot are up to date.
///
/// @param chroot_parent The path to the parent directory where the chroot will be created.
///        The chroot root directory itself will be created as a subdirectory named "root" within
/// this parent directory(e.g., if `chroot_parent` is "/path/to/chroots", the chroot root
/// will be at "/path/to/chroots/root").
///
/// @param makepkgconf_path The path to the `makepkg.conf` file on the host system that should be
/// copied into the chroot environment. This configuration file will be used by `makepkg`
/// inside the chroot to define build settings.
///
/// @param pacmanconf_path The path to the `pacman.conf` file on the host system that should be
/// copied into the chroot environment. This configuration file will be used by `pacman`
/// inside the chroot to configure package manager settings, including repositories and cache
/// locations.
///
/// @param build_paccachedir An optional path to a custom pacman cache directory on the host system.
/// If provided, this directory will be used as a shared package cache for `pacman` within
/// the chroot. This can speed up operations by reusing downloaded packages. If not provided,
/// `pacman` will use its default cache directory within the chroot, which is separate from
/// the host system's cache.
///
/// TODO add description for packages param
/// @param packages .
///
/// @return `Ok` if the chroot environment was successfully set up or updated, `Err` otherwise.
///         Possible reasons for failure include:
///         - Insufficient permissions to create directories or copy files.
///         - Errors during the execution of `mkarchroot` or `arch-nspawn`.
///         - Invalid paths provided for configuration files or the chroot parent directory.
pub fn setup_chroot(
    chroot_parent: &str,
    makepkgconf_path: &str,
    pacmanconf_path: &str,
    build_paccachedir: Option<String>,
    packages: Option<Vec<String>>,
) -> Result<()> {
    if let Some(build_paccachedir) = &build_paccachedir {
        fs::create_dir_all(build_paccachedir).context("failed to create paccachedir")?;
    }

    let chroot_params = ChrootParams {
        chroot_root_path: format!("{chroot_parent}/root"),
        makepkgconf_path: makepkgconf_path.into(),
        pacmanconf_path: pacmanconf_path.into(),
        build_paccachedir,
        packages,
    };

    if !Path::new(&chroot_params.chroot_root_path).exists() {
        fs::create_dir_all(chroot_parent).map_err(|err| {
            anyhow::anyhow!(
                "failed to create chroot parent dir '{chroot_parent}': {err}. Please create it \
                 and make it owned by the current user",
            )
        })?;

        // 1. run mkarchroot to create root chroot
        let mkarchroot_args = construct_mkarchroot_args(&chroot_params);
        let res = subproc::exec_cmd("mkarchroot", &mkarchroot_args, None, true)
            .context("failed to run mkarchroot")?;
        if res.exit_code != 0 {
            anyhow::bail!("failed to create root chroot: {}", res.output);
        }

        // 2. additionally copy pacman.conf into created chroot dir
        let dest_chroot_pacmanconf = format!("{}/etc/pacman.conf", chroot_params.chroot_root_path);
        let res = subproc::exec_cmd(
            "sudo",
            &["cp".into(), chroot_params.pacmanconf_path, dest_chroot_pacmanconf],
            None,
            true,
        )
        .context("failed to copy pacmanconf")?;
        if res.exit_code != 0 {
            anyhow::bail!("error copying pacman.conf: {}", res.output);
        }
        return Ok(());
    }

    let nspawn_args = construct_nspawn_args(&chroot_params);
    let res = subproc::exec_cmd("arch-nspawn", &nspawn_args, None, true)
        .context("failed to run arch-nspawn")?;
    if res.exit_code != 0 {
        anyhow::bail!("failed to update chroot: {}", res.output);
    }
    Ok(())
}

/// @brief Fetches Arch Linux package source files using Git.
///
/// This function clones or updates the official Arch Linux packaging Git repository
/// for the specified package base from gitlab.archlinux.org. It then checks out
/// the specific version by `tagver`.
///
/// @param pkgbase The base name of the package (e.g., "linux") used to construct the Git repository
/// URL.
///
/// @param tagver The Git tag identifying the specific package version source to
/// retrieve.
///
/// @param dest_path The local filesystem path where the Git repository should be cloned.
///
/// @return True if the Git operations were successful, false otherwise.
pub fn fetch_archpkgbuild<PathLike: AsRef<Path>>(
    pkgbase: &str,
    tagver: &str,
    dest_path: PathLike,
    proxy_url: Option<String>,
) -> Result<bool> {
    // make sure the dest path is absolute
    let dest_path = path::absolute(dest_path).context("failed to absolutize dest path")?;

    // shouldn't be able to fetch in current dir
    let current_dir = env::current_dir().context("failed to get current dir")?;
    if current_dir == dest_path {
        anyhow::bail!("Current directory cannot be used as destination folder");
    }

    // for safety reasons dest shouldn't contain .git
    if !dest_path.exists() || (dest_path.exists() && !dest_path.join(".git").exists()) {
        let git_url =
            format!("https://gitlab.archlinux.org/archlinux/packaging/packages/{pkgbase}.git");

        // simple clone
        archgit_utils::git_repo_clone(&git_url, None, None, &dest_path, proxy_url)
            .context("failed to clone repo")?;

        // double check
        let ret_ex_proc = subproc::exec_cmd(
            "git",
            &["checkout".into(), tagver.into()],
            Some(dest_path.to_str().unwrap().into()),
            true,
        )?;
        if ret_ex_proc.exit_code != 0 {
            error!("[GIT] failed to run git checkout: {}", ret_ex_proc.output);
            return Ok(false);
        }
    } else {
        anyhow::bail!("Destination path cannot be existing git repository");
    }
    Ok(true)
}

/// @brief Clean the chroot directory.
///
/// This function cleans up the chroot directory by
/// removing temporary files and directories.
///
/// @param chroot_dir The path to chroot.
///
/// @return True if the cleaning was successful, false otherwise.
pub fn clean_chroot_dir(chroot_dir: &str) -> bool {
    if !chroot_dir.is_empty() && Path::new(chroot_dir).exists() && Path::new(chroot_dir).is_dir() {
        // NOTE: chroot is created with root user perms
        if let Err(err) =
            subproc::exec_cmd("sudo", &["rm".into(), "-rf".into(), chroot_dir.into()], None, true)
        {
            error!("failed to remove chroot folder({chroot_dir}): {err}");
            return false;
        }
        let lockfile = format!("{chroot_dir}.lock");
        if let Err(err) = fs::remove_file(&lockfile) {
            debug!("failed to remove chroot_dir({lockfile}): {err}");
        }
    }
    true
}
