// SPDX-FileCopyrightText: 2026 Phosh.mobi e.V.
// SPDX-License-Identifier: GPL-3.0-or-later

use std::fmt;

#[derive(Debug)]
pub struct UpdateInfo {
    pub version: String,
    pub description: String,
}

#[derive(Debug)]
pub enum UpdateError {
    // TODO: more info why update check failed?
    Failed(String),
}

impl fmt::Display for UpdateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Failed(msg) => write!(f, "{msg}"),
        }
    }
}
impl std::error::Error for UpdateError {}
