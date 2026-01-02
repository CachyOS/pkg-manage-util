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

mod build;
mod git;

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Default, Deserialize, Serialize)]
pub struct Config {
    chroot_dir: Option<PathBuf>,
    makepkgconf_path: Option<PathBuf>,
    pacmanconf_path: Option<PathBuf>,
    build: Option<build::BuildConfig>,
    git: Option<git::GitConfig>,
}

impl Config {
    pub fn load() -> Self {
        let mut config = Self::default();
        let config_paths = [
            xdg::BaseDirectories::with_prefix("pkg-manage-util").get_config_file("config.toml"),
            xdg::BaseDirectories::new().get_config_file("pkg-manage-util.toml"),
        ];
        for check_path in config_paths.into_iter().flatten() {
            if let Ok(config_file) = Self::parse_from_file(&check_path) {
                config = config_file;
                break;
            }
        }
        config
    }

    pub fn parse_from_file<P: AsRef<Path>>(filepath: P) -> Result<Self> {
        let file_content =
            fs::read_to_string(filepath.as_ref()).context("failed to read config")?;
        if file_content.is_empty() {
            anyhow::bail!("The config file is empty!")
        }

        let config: Self = toml::from_str(&file_content)?;
        Ok(config)
    }

    pub fn chroot_dir(&self) -> PathBuf {
        self.chroot_dir.clone().unwrap_or_else(|| PathBuf::from("/var/lib/pkg-manage-util/chroots"))
    }

    pub fn gpg_key(&self) -> Option<String> {
        self.build.as_ref().and_then(build::BuildConfig::gpg_key)
    }

    pub fn makepkgconf_path(&self) -> PathBuf {
        self.makepkgconf_path
            .clone()
            .unwrap_or_else(|| PathBuf::from("/usr/share/devtools/makepkg.conf.d/x86_64.conf"))
    }

    pub fn pacmanconf_path(&self) -> PathBuf {
        self.pacmanconf_path
            .clone()
            .unwrap_or_else(|| PathBuf::from("/usr/share/devtools/pacman.conf.d/multilib.conf"))
    }

    pub fn build_paccachedir(&self) -> Option<PathBuf> {
        self.build.as_ref().and_then(build::BuildConfig::build_paccachedir)
    }

    pub fn timeout(&self) -> Option<u64> {
        self.build.as_ref().and_then(build::BuildConfig::timeout)
    }

    pub fn proxy_url(&self) -> Option<String> {
        self.git.as_ref().and_then(git::GitConfig::proxy_url).clone()
    }

    pub fn dump_config(&self) -> Result<String> {
        let actual_config = Self {
            chroot_dir: Some(self.chroot_dir()),
            makepkgconf_path: Some(self.makepkgconf_path()),
            pacmanconf_path: Some(self.pacmanconf_path()),
            build: self.build.as_ref().map(build::BuildConfig::actual_config),
            git: self.git.as_ref().map(git::GitConfig::actual_config),
        };
        let toml_content = toml::to_string(&actual_config)?;
        Ok(toml_content)
    }
}
