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

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Default, Deserialize, Serialize)]
pub struct BuildConfig {
    gpg_key: Option<String>,
    build_paccachedir: Option<PathBuf>,
    timeout: Option<u64>,
}

impl BuildConfig {
    pub fn gpg_key(&self) -> Option<String> {
        self.gpg_key.clone()
    }

    pub fn build_paccachedir(&self) -> Option<PathBuf> {
        self.build_paccachedir.clone()
    }

    pub fn timeout(&self) -> Option<u64> {
        self.timeout.clone()
    }

    pub fn actual_config(&self) -> Self {
        Self {
            gpg_key: self.gpg_key(),
            build_paccachedir: self.build_paccachedir(),
            timeout: self.timeout(),
        }
    }
}
