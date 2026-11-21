//! Remote files over SFTP through `yazi-sftp` (yazi's russh-sftp fork, the one yazi crate with no Lua
//! in it). Connection details come from `ssh -G <host>`, so aliases in `~/.ssh/config` work and
//! nothing is hardcoded; host keys are checked against `known_hosts` the way OpenSSH does, which
//! yazi itself skips.

use crate::entry::{Entry, Kind};
use russh::client::{self, Handler};
use russh::keys::{PrivateKeyWithHashAlg, PublicKeyOrCertificate};
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use yazi_sftp::fs::{Attrs, Flags};

pub const SCHEME: &str = "sftp://";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Url {
    pub user: Option<String>,
    pub host: String,
    pub port: Option<u16>,
    /// Absolute remote path, `/` when the URL has none.
    pub path: String,
}

pub fn is_url(s: &str) -> bool {
    s.starts_with(SCHEME)
}

impl Url {
    /// `sftp://[user@]host[:port][/path]`, with `[v6]` literals accepted.
    pub fn parse(s: &str) -> Result<Self, String> {
        let rest = s.strip_prefix(SCHEME).ok_or_else(|| format!("`{s}`: not an sftp:// URL"))?;
        let (authority, path) = match rest.find('/') {
            Some(i) => (&rest[..i], &rest[i..]),
            None => (rest, "/"),
        };
        let (user, hostport) = match authority.rsplit_once('@') {
            Some((u, h)) if !u.is_empty() => (Some(u.to_string()), h),
            Some((_, h)) => (None, h),
            None => (None, authority),
        };
        let (host, port) = if let Some(v6) = hostport.strip_prefix('[') {
            let (h, after) = v6.split_once(']').ok_or_else(|| format!("`{s}`: unclosed `[` in host"))?;
            (h, after.strip_prefix(':'))
        } else {
            match hostport.rsplit_once(':') {
                Some((h, p)) => (h, Some(p)),
                None => (hostport, None),
            }
        };
        if host.is_empty() {
            return Err(format!("`{s}`: missing host"));
        }
        let port = port.filter(|p| !p.is_empty()).map(|p| p.parse::<u16>().map_err(|_| format!("`{s}`: bad port `{p}`"))).transpose()?;
        Ok(Self { user, host: host.to_string(), port, path: path.to_string() })
    }
}

/// What `ssh -G` resolved for a host.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SshTarget {
    pub host: String,
    pub user: Option<String>,
    pub port: u16,
    pub identity_files: Vec<PathBuf>,
    pub known_hosts: Vec<PathBuf>,
}

fn expand(p: &str) -> PathBuf {
    match p.strip_prefix("~/").or_else(|| p.strip_prefix("~\\")) {
        Some(rest) => dirs::home_dir().map(|h| h.join(rest)).unwrap_or_else(|| PathBuf::from(p)),
        None => PathBuf::from(p),
    }
}

/// Parse the output of `ssh -G`: lowercase `key value` lines, repeated keys for lists.
pub fn parse_ssh_g(out: &str) -> SshTarget {
    let mut t = SshTarget { port: 22, ..SshTarget::default() };
    for line in out.lines() {
        let Some((key, value)) = line.split_once(' ') else { continue };
        let value = value.trim();
        match key {
            "hostname" => t.host = value.to_string(),
            "user" => t.user = Some(value.to_string()),
            "port" => t.port = value.parse().unwrap_or(22),
            "identityfile" => t.identity_files.push(expand(value)),
            "userknownhostsfile" => t.known_hosts.extend(value.split_whitespace().filter(|p| *p != "none").map(expand)),
            _ => {}
        }
    }
    t
}

/// Resolve through the local `ssh` binary; without it the URL's own values and OpenSSH's default key names apply.
pub fn resolve(url: &Url) -> SshTarget {
    let mut cmd = std::process::Command::new("ssh");
    cmd.arg("-G");
    if let Some(u) = &url.user {
        cmd.args(["-l", u]);
    }
    if let Some(p) = url.port {
        cmd.args(["-p", &p.to_string()]);
    }
    cmd.arg(&url.host);
    let mut t = match cmd.output() {
        Ok(o) if o.status.success() => parse_ssh_g(&String::from_utf8_lossy(&o.stdout)),
        _ => SshTarget { port: url.port.unwrap_or(22), user: url.user.clone(), ..SshTarget::default() },
    };
    if t.host.is_empty() {
        t.host = url.host.clone();
    }
    if t.identity_files.is_empty()
        && let Some(home) = dirs::home_dir()
    {
        t.identity_files = ["id_ed25519", "id_ecdsa", "id_rsa"].iter().map(|n| home.join(".ssh").join(n)).collect();
    }
    if t.known_hosts.is_empty()
        && let Some(home) = dirs::home_dir()
    {
        t.known_hosts = vec![home.join(".ssh").join("known_hosts")];
    }
    t
}

#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Use this key instead of the ones `ssh -G` lists.
    pub identity: Option<PathBuf>,
    pub passphrase: Option<String>,
    /// Record an unknown host key instead of refusing it (OpenSSH `StrictHostKeyChecking=accept-new`).
    pub accept_new: bool,
}

struct Client {
    /// Names the server may be recorded under: the alias typed by the user and the resolved host name.
    names: Vec<String>,
    port: u16,
    known_hosts: Vec<PathBuf>,
    accept_new: bool,
}

impl Handler for Client {
    type Error = russh::Error;

    async fn check_server_key(&mut self, key: &PublicKeyOrCertificate) -> Result<bool, Self::Error> {
        let PublicKeyOrCertificate::PublicKey { key, .. } = key else {
            return Ok(false); // host certificates are not supported
        };
        for file in &self.known_hosts {
            for name in &self.names {
                // A changed key is an error, an unknown host is `false`; a match on any name wins.
                if russh::keys::known_hosts::check_known_hosts_path(name, self.port, key, file)? {
                    return Ok(true);
                }
            }
        }
        if self.accept_new
            && let (Some(file), Some(name)) = (self.known_hosts.first(), self.names.first())
        {
            russh::keys::known_hosts::learn_known_hosts_path(name, self.port, key, file)?;
            return Ok(true);
        }
        Ok(false)
    }
}

pub struct Remote {
    op: yazi_sftp::Operator,
    _session: client::Handle<Client>,
}

fn other(context: &str, e: impl std::fmt::Display) -> io::Error {
    io::Error::other(format!("{context}: {e}"))
}

impl Remote {
    pub async fn connect(url: &Url, opts: &Options) -> io::Result<Self> {
        let target = resolve(url);
        let user = url.user.clone().or(target.user.clone()).or_else(|| std::env::var("USER").or_else(|_| std::env::var("USERNAME")).ok()).ok_or_else(|| io::Error::other("no user: put one in the URL or in ssh config"))?;
        let config = Arc::new(client::Config { inactivity_timeout: Some(Duration::from_secs(60)), keepalive_interval: Some(Duration::from_secs(10)), ..Default::default() });
        let mut names = vec![url.host.clone()];
        if target.host != url.host {
            names.push(target.host.clone());
        }
        let handler = Client { names, port: target.port, known_hosts: target.known_hosts.clone(), accept_new: opts.accept_new };
        let connect = client::connect(config, (target.host.as_str(), target.port), handler);
        let mut session = tokio::time::timeout(Duration::from_secs(20), connect)
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, format!("{}:{}: connect timed out", target.host, target.port)))?
            .map_err(|e| other(&format!("{}:{}: connect failed (unknown host key? try --accept-new)", target.host, target.port), e))?;

        let candidates: Vec<PathBuf> = match &opts.identity {
            Some(p) => vec![p.clone()],
            None => target.identity_files.iter().filter(|p| p.is_file()).cloned().collect(),
        };
        let mut tried = Vec::new();
        let mut authed = false;
        for path in &candidates {
            let key = match russh::keys::load_secret_key(path, opts.passphrase.as_deref()) {
                Ok(k) => k,
                Err(e) => {
                    tried.push(format!("{}: {e}", path.display()));
                    continue;
                }
            };
            let hash = session.best_supported_rsa_hash().await.map_err(|e| other("negotiating signature hash", e))?.flatten();
            let result = session.authenticate_publickey(user.clone(), PrivateKeyWithHashAlg::new(Arc::new(key), hash)).await.map_err(|e| other("authentication", e))?;
            if result.success() {
                authed = true;
                break;
            }
            tried.push(format!("{}: rejected", path.display()));
        }
        if !authed {
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, format!("{user}@{}: no key accepted [{}]", target.host, tried.join("; "))));
        }

        let channel = session.channel_open_session().await.map_err(|e| other("opening channel", e))?;
        channel.request_subsystem(true, "sftp").await.map_err(|e| other("starting the sftp subsystem", e))?;
        let mut op = yazi_sftp::Operator::make(channel.into_stream());
        op.init().await?;
        Ok(Self { op, _session: session })
    }

    pub async fn list(&self, dir: &str) -> io::Result<Vec<Entry>> {
        let mut rd = self.op.read_dir(dir).await?;
        let mut out = Vec::new();
        while let Some(item) = rd.next().await? {
            out.push(entry_from_attrs(dir, item.name(), item.attrs()));
        }
        Ok(out)
    }

    pub async fn stat(&self, path: &str) -> io::Result<Entry> {
        let attrs = self.op.stat(path).await?;
        let (dir, name) = path.trim_end_matches('/').rsplit_once('/').unwrap_or(("", path));
        Ok(entry_from_attrs(if dir.is_empty() { "/" } else { dir }, name.as_bytes(), &attrs))
    }

    /// First `max` bytes of a remote file, and whether more follows.
    pub async fn read_head(&self, path: &str, max: usize) -> io::Result<(Vec<u8>, bool)> {
        let mut file = self.op.open(path, Flags::READ, &Attrs::default()).await?;
        let mut buf = Vec::with_capacity(max.min(1 << 20) + 1);
        (&mut file).take(max as u64 + 1).read_to_end(&mut buf).await?;
        let more = buf.len() > max;
        buf.truncate(max);
        Ok((buf, more))
    }
}

/// Map SFTP attributes to an [`Entry`]; the file type comes from the POSIX mode bits.
pub fn entry_from_attrs(dir: &str, name: &[u8], attrs: &Attrs) -> Entry {
    const S_IFMT: u32 = 0o170000;
    let name = String::from_utf8_lossy(name).into_owned();
    let mode = attrs.perm.unwrap_or(0);
    let kind = match mode & S_IFMT {
        0o040000 => Kind::Dir,
        0o120000 => Kind::Symlink,
        0o100000 | 0 => Kind::File,
        _ => Kind::Other,
    };
    let path = if dir.ends_with('/') { format!("{dir}{name}") } else { format!("{dir}/{name}") };
    Entry {
        path,
        hidden: name.starts_with('.'),
        name,
        kind,
        size: if kind == Kind::Dir { 0 } else { attrs.size.unwrap_or(0) },
        mtime_ms: attrs.mtime.map(|t| i64::from(t) * 1000),
        readonly: attrs.perm.is_some() && mode & 0o222 == 0,
        executable: kind == Kind::File && mode & 0o111 != 0,
        link: None,
        broken: false,
        mime: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_urls() {
        let u = Url::parse("sftp://deploy@fr1.example:2222/var/log").unwrap();
        assert_eq!((u.user.as_deref(), u.host.as_str(), u.port, u.path.as_str()), (Some("deploy"), "fr1.example", Some(2222), "/var/log"));
        let u = Url::parse("sftp://fr1").unwrap();
        assert_eq!((u.user, u.host.as_str(), u.port, u.path.as_str()), (None, "fr1", None, "/"));
        let u = Url::parse("sftp://me@[::1]:22/tmp").unwrap();
        assert_eq!((u.host.as_str(), u.port), ("::1", Some(22)));
        assert!(Url::parse("sftp://").is_err());
        assert!(Url::parse("sftp://h:99999/").is_err());
        assert!(Url::parse("http://h/").is_err());
        assert!(is_url("sftp://x") && !is_url("/tmp"));
    }

    #[test]
    fn parses_ssh_g_output() {
        let out = "user ubuntu\nhostname 10.0.0.5\nport 2200\nidentityfile ~/.ssh/key_a\nidentityfile /abs/key_b\nuserknownhostsfile /k/one ~/.ssh/two\nforwardagent no\n";
        let t = parse_ssh_g(out);
        assert_eq!((t.host.as_str(), t.user.as_deref(), t.port), ("10.0.0.5", Some("ubuntu"), 2200));
        assert_eq!(t.identity_files.len(), 2);
        assert!(t.identity_files[0].ends_with(".ssh/key_a") || t.identity_files[0].ends_with(".ssh\\key_a"));
        assert_eq!(t.identity_files[1], PathBuf::from("/abs/key_b"));
        assert_eq!(t.known_hosts.len(), 2);
        assert_eq!(parse_ssh_g("hostname h\n").port, 22);
        assert!(parse_ssh_g("userknownhostsfile none\n").known_hosts.is_empty());
    }

    #[test]
    fn attrs_become_entries() {
        let dir = Attrs { perm: Some(0o040755), mtime: Some(1_700_000_000), ..Attrs::default() };
        let e = entry_from_attrs("/srv", b"www", &dir);
        assert_eq!((e.kind, e.path.as_str(), e.size, e.mtime_ms, e.executable), (Kind::Dir, "/srv/www", 0, Some(1_700_000_000_000), false));

        let file = Attrs { perm: Some(0o100755), size: Some(42), ..Attrs::default() };
        let e = entry_from_attrs("/srv/", b".run.sh", &file);
        assert_eq!((e.kind, e.path.as_str(), e.size, e.hidden, e.executable, e.readonly), (Kind::File, "/srv/.run.sh", 42, true, true, false));

        let link = Attrs { perm: Some(0o120777), ..Attrs::default() };
        assert_eq!(entry_from_attrs("/", b"l", &link).kind, Kind::Symlink);

        let ro = Attrs { perm: Some(0o100444), ..Attrs::default() };
        assert!(entry_from_attrs("/", b"r", &ro).readonly);
        assert_eq!(entry_from_attrs("/", b"unknown", &Attrs::default()).kind, Kind::File);
    }

    #[test]
    fn invalid_utf8_names_do_not_panic() {
        let e = entry_from_attrs("/d", &[b'a', 0xff, b'b'], &Attrs::default());
        assert!(e.name.starts_with('a') && e.name.ends_with('b'));
    }
}
