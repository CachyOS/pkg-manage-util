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

use tracing_subscriber::EnvFilter;
use tracing_subscriber::prelude::*;

pub fn init_logger() {
    // set log level from RUST_LOG env var
    let env_filter = EnvFilter::try_from_default_env();

    // create subscriber env filter
    let subscriber_env_filter = env_filter.unwrap_or_else(|_| EnvFilter::new("info"));

    // create stdout layer
    let stdout_log =
        tracing_subscriber::fmt::layer().without_time().compact().with_writer(std::io::stdout);

    tracing_subscriber::registry().with(stdout_log).with(subscriber_env_filter).init();
}
