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

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, PartialEq, Debug)]
#[command(author, version, about, long_about = None)]
#[clap(subcommand_negates_reqs = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Parser, PartialEq, Debug)]
pub struct ArchCloneCli {
    /// The package to clone
    pub pkgbase: String,

    /// The package version to clone
    pub version: Option<String>,
}

#[derive(Parser, PartialEq, Debug)]
pub struct AurCloneCli {
    /// The package to clone
    pub pkgbase: String,

    /// The depth of repo to clone
    #[arg(long)]
    pub depth: Option<i32>,
}

#[derive(Parser, PartialEq, Debug)]
pub struct GitCloneCli {
    /// The repo url to clone
    pub git_url: String,

    /// The destination path to clone
    pub dest_path: PathBuf,

    /// The depth of repo to clone
    #[arg(long)]
    pub depth: Option<i32>,

    /// The repo branch to clone
    #[arg(short, long)]
    pub branch: Option<String>,

    /// The flag used to clone single branch remote
    #[arg(long)]
    pub single_branch: bool,
}

#[derive(Parser, PartialEq, Debug)]
pub struct BuildCli {
    /// The path to the PKGBUILD file
    pub pkgbuild_path: Option<PathBuf>,
}

#[derive(Subcommand, PartialEq, Debug)]
pub enum Commands {
    /// Clones Archlinux repository
    #[command(arg_required_else_help = true)]
    CloneArchRepo(ArchCloneCli),
    /// Clones AUR repository
    #[command(arg_required_else_help = true)]
    CloneAurRepo(AurCloneCli),
    /// Clones git repository
    #[command(arg_required_else_help = true)]
    CloneGitRepo(GitCloneCli),
    /// Builds a package
    Build(BuildCli),
    /// Dumps used config file
    #[command()]
    DumpConfig,
}
