use std::{borrow::Cow, fmt};

use crate::config;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteName(Cow<'static, str>);

impl fmt::Display for RemoteName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl RemoteName {
    pub fn new(from: String) -> Self {
        Self(Cow::Owned(from))
    }

    pub const fn new_static(from: &'static str) -> Self {
        Self(Cow::Borrowed(from))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        match self.0 {
            Cow::Borrowed(s) => s.to_owned(),
            Cow::Owned(s) => s,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteUrl(String);

impl fmt::Display for RemoteUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl RemoteUrl {
    pub fn new(from: String) -> Self {
        Self(from)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum RemoteType {
    Ssh,
    Https,
    File,
}

impl From<config::RemoteType> for RemoteType {
    fn from(value: config::RemoteType) -> Self {
        match value {
            config::RemoteType::Ssh => Self::Ssh,
            config::RemoteType::Https => Self::Https,
            config::RemoteType::File => Self::File,
        }
    }
}

impl From<RemoteType> for config::RemoteType {
    fn from(value: RemoteType) -> Self {
        match value {
            RemoteType::Ssh => Self::Ssh,
            RemoteType::Https => Self::Https,
            RemoteType::File => Self::File,
        }
    }
}
