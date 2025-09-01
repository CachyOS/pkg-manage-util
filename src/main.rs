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

mod actions;
mod args;
mod config;
mod logger;

use args::*;
use config::Config;

use anyhow::Result;
use clap::Parser;

fn main() -> Result<()> {
    let args = Cli::parse();

    // initialize the logger
    logger::init_logger();

    // load config
    let config = Config::load();

    match &args.command {
        Commands::DumpConfig => {
            actions::dump_config(&config)?;
        },
        Commands::CloneArchRepo(args) => {
            actions::clone_arch_repo(&config, args)?;
        },
        Commands::CloneAurRepo(args) => {
            actions::clone_aur_repo(&config, args)?;
        },
        Commands::CloneGitRepo(args) => {
            actions::clone_git_repo(&config, args)?;
        },
        Commands::Build(args) => {
            actions::build_pkg(&config, args)?;
        },
    }

    Ok(())
}
