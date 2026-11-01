use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct HostSet {
    hosts: BTreeSet<String>,
}

impl HostSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_hosts<I, S>(hosts: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut set = Self::new();
        for host in hosts {
            set.insert(host.as_ref());
        }
        set
    }

    pub fn insert(&mut self, host: &str) -> bool {
        match normalize_host(host) {
            Some(host) => self.hosts.insert(host),
            None => false,
        }
    }

    pub fn contains(&self, host: &str) -> bool {
        normalize_host(host).is_some_and(|host| self.hosts.contains(&host))
    }

    pub fn allows_host(&self, host: &str) -> bool {
        let Some(host) = normalize_host(host) else {
            return false;
        };
        self.hosts.contains(&host) || self.hosts.iter().any(|parent| is_subdomain(&host, parent))
    }

    pub fn allows_url(&self, url: &str, require_https: bool) -> bool {
        host_of(url, require_https).is_some_and(|host| self.allows_host(&host))
    }

    pub fn len(&self) -> usize {
        self.hosts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.hosts.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &String> {
        self.hosts.iter()
    }
}

pub fn host_of(url: &str, require_https: bool) -> Option<String> {
    let parsed = Url::parse(url).ok()?;
    match parsed.scheme() {
        "https" => {}
        "http" if !require_https => {}
        _ => return None,
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return None;
    }
    normalize_host(parsed.host_str()?)
}

pub fn normalize_host(host: &str) -> Option<String> {
    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() {
        return None;
    }
    let host = match host.rsplit_once(':') {
        Some((head, port)) if !head.contains(':') && port.chars().all(|c| c.is_ascii_digit()) => {
            head.to_string()
        }
        _ => host,
    };
    if host.is_empty()
        || host.contains('/')
        || host.contains('@')
        || host.contains('*')
        || host.starts_with('.')
    {
        return None;
    }
    Some(host)
}

fn is_subdomain(host: &str, parent: &str) -> bool {
    host.len() > parent.len() + 1
        && host.ends_with(parent)
        && host.as_bytes()[host.len() - parent.len() - 1] == b'.'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_exact_and_subdomains_only() {
        let hosts = HostSet::from_hosts(["upload.wikimedia.org"]);
        assert!(hosts.allows_host("upload.wikimedia.org"));
        assert!(hosts.allows_host("cache.upload.wikimedia.org"));
        assert!(!hosts.allows_host("notupload.wikimedia.org"));
        assert!(!hosts.allows_host("wikimedia.org"));
    }

    #[test]
    fn rejects_credentials_and_plain_http() {
        let hosts = HostSet::from_hosts(["upload.wikimedia.org"]);
        assert!(!hosts.allows_url("https://user:pw@upload.wikimedia.org/x.gif", true));
        assert!(!hosts.allows_url("http://upload.wikimedia.org/x.gif", true));
        assert!(hosts.allows_url("http://upload.wikimedia.org/x.gif", false));
        assert!(!hosts.allows_url("file:///x.gif", false));
    }
}
