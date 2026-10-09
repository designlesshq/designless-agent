//! The shared, cwd-independent canvas-edit probe the wake hooks read.
//!
//! Discovery is keyed on the user, not the directory: the desktop app answers
//! `list_inbox` over its local socket, so a waiting edit is found from any
//! working directory. When the socket is absent the probe reports empty (no
//! canvas running means nothing to miss). When it is present but slow, denied
//! or stale the probe reports UNKNOWN, never empty: "could not ask" and
//! "nothing waiting" are different answers, and every caller says which.
//!
//! Fail-open throughout: nothing here blocks a turn. Fail-open means "never
//! block", not "report all clear".

use super::js::{self, J};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const TIMEOUT_MS: u64 = 700;

/// What a missed quick check means, said in every line that reports one.
///
/// The budget is short on purpose (it runs on every prompt) and a healthy
/// desktop misses it often. A reproduction of the miss is evidence of
/// behaviour, never of a defect. The clause is short because the watcher's
/// line has a byte ceiling that only comes down.
pub const MISSED_HINT: &str = "By design, not a fault: nothing to fix or report.";

/// The probe's answer. `unknown` is `None` when the answer is trustworthy and
/// a short reason when nothing could be determined. Callers MUST tell the two
/// apart: zero with no reason is "nothing waiting", zero with a reason is
/// "could not find out".
#[derive(Debug, Clone, Default)]
pub struct Probe {
    pub sessions: Vec<J>,
    pub attn_dark: Option<f64>,
    pub unknown: Option<String>,
}

impl Probe {
    pub fn count(&self) -> usize {
        self.sessions.len()
    }
    fn unknown(reason: impl Into<String>) -> Self {
        Probe { unknown: Some(reason.into()), ..Default::default() }
    }
}

// ── server/IPC input validation (trust boundary) ───────────────────────────
// safety_branch and repo_remote arrive from the desktop and are embedded
// verbatim into git instructions handed to the agent, so they are validated
// here and a malformed value never reaches that text.

/// `^designless\/[A-Za-z0-9._/-]+$`: a branch in the server-owned namespace,
/// safe characters only.
pub fn is_safe_branch_name(b: Option<&J>) -> bool {
    match b {
        Some(J::Str(s)) => s
            .strip_prefix("designless/")
            .is_some_and(|rest| !rest.is_empty() && rest.chars().all(|c| c.is_ascii_alphanumeric() || "._/-".contains(c))),
        _ => false,
    }
}

/// A repo remote with no shell metacharacters, in a shape the normaliser can
/// fold. The character class is the injection defence; the shapes are
/// recognition, and recognition is never narrower than `normalize_remote`.
pub fn is_safe_repo_remote(r: Option<&J>) -> bool {
    let Some(J::Str(raw)) = r else { return false };
    let s = js::trim(raw);
    // A space is a legitimate path character, made safe by quoting where the
    // value is embedded. Newlines and tabs are not path characters.
    if s.is_empty() || s.chars().any(|c| "\n\r\t;&$()|<>`'\"\\".contains(c)) {
        return false;
    }
    let word = |c: char| js::is_word(c);
    // any scheme normalize_remote strips: ^[a-z][a-z0-9+.-]*:\/\/[\w.@:/~ -]+$  (i)
    if let Some(colon) = s.find(':') {
        let scheme = &s[..colon];
        let rest = &s[colon..];
        let mut sc = scheme.chars();
        if sc.next().is_some_and(|c| c.is_ascii_alphabetic())
            && sc.all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c))
        {
            if let Some(tail) = rest.strip_prefix("://") {
                if !tail.is_empty() && tail.chars().all(|c| word(c) || ".@:/~ -".contains(c)) {
                    return true;
                }
            }
        }
    }
    // scp-like: ^[\w.-]+@[\w.@:/~-]+$
    if let Some(at) = s.find('@') {
        let (user, host) = (&s[..at], &s[at + 1..]);
        if !user.is_empty()
            && user.chars().all(|c| word(c) || ".-".contains(c))
            && !host.is_empty()
            && host.chars().all(|c| word(c) || ".@:/~-".contains(c))
        {
            return true;
        }
    }
    // owner/repo shorthand: ^[\w.-]+\/[\w.-]+$
    if let Some((a, b)) = s.split_once('/') {
        let part = |p: &str| !p.is_empty() && p.chars().all(|c| word(c) || ".-".contains(c));
        if part(a) && part(b) {
            return true;
        }
    }
    false
}

/// Drop any row carrying a PRESENT-but-malformed identifier. An absent or null
/// one is allowed through; a present malformed one is not surfaced at all.
pub fn sanitize_inbox_rows(rows: Option<&J>) -> Vec<J> {
    let Some(J::Arr(rows)) = rows else { return Vec::new() };
    rows.iter()
        .filter(|s| {
            // `!s || typeof s !== 'object'`: arrays are objects in the language.
            if !matches!(s, J::Obj(_) | J::Arr(_)) {
                return false;
            }
            let b = js::get(Some(s), "safety_branch");
            if !js::nullish(b) && !is_safe_branch_name(b) {
                return false;
            }
            let r = js::get(Some(s), "repo_remote");
            if !js::nullish(r) && !is_safe_repo_remote(r) {
                return false;
            }
            true
        })
        .cloned()
        .collect()
}

/// The dark count: needs-attention items the canvas has not shown for more
/// than a day. `None` means the desktop did not say, which is not zero.
pub fn dark_count(v: Option<&J>) -> Option<f64> {
    match v {
        Some(J::Num(n)) if n.is_finite() && *n >= 0.0 => Some(n.floor()),
        Some(J::Str(s)) if !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) => Some(js::str_to_number(s)),
        _ => None,
    }
}

/// The desktop socket and its directory: `DESIGNLESS_IPC_SOCKET` when it names
/// one, else the per-user default the bridge and every shipped app use.
#[derive(Debug, PartialEq)]
pub struct SockPath {
    pub dir: String,
    pub sock: String,
}

pub fn socket_path() -> Option<SockPath> {
    socket_path_from(
        std::env::var("DESIGNLESS_IPC_SOCKET").ok().as_deref(),
        std::env::var("XDG_RUNTIME_DIR").ok().as_deref(),
    )
}

pub fn socket_path_from(explicit: Option<&str>, xdg: Option<&str>) -> Option<SockPath> {
    let explicit = js::trim(explicit.unwrap_or(""));
    if !explicit.is_empty() {
        return Some(SockPath { dir: js::dirname(explicit), sock: explicit.to_string() });
    }
    // SAFETY: getuid is infallible and thread-safe.
    let uid = unsafe { libc::getuid() };
    let per_user = || {
        let dir = format!("/tmp/designless-{uid}");
        SockPath { sock: js::join(&[&dir, "ipc.sock"]), dir }
    };
    if cfg!(target_os = "macos") {
        return Some(per_user());
    }
    if let Some(x) = xdg.filter(|x| !x.is_empty()) {
        let dir = js::join(&[x, "Designless"]);
        return Some(SockPath { sock: js::join(&[&dir, "ipc.sock"]), dir });
    }
    Some(per_user())
}

/// Refuse a socket dir that is not owner-only (a same-uid-malware guard).
fn dir_is_safe(dir: &str) -> bool {
    use std::os::unix::fs::MetadataExt;
    match std::fs::metadata(dir) {
        Ok(md) => md.is_dir() && md.uid() == unsafe { libc::getuid() } && (md.mode() & 0o077) == 0,
        Err(_) => false,
    }
}

/// The errno name Node prints in a socket error message.
fn errno_name(e: &std::io::Error) -> String {
    let Some(n) = e.raw_os_error() else { return "UNKNOWN".into() };
    let names: &[(i32, &str)] = &[
        (libc::ECONNREFUSED, "ECONNREFUSED"),
        (libc::ENOENT, "ENOENT"),
        (libc::EACCES, "EACCES"),
        (libc::EPERM, "EPERM"),
        (libc::ENOTSOCK, "ENOTSOCK"),
        (libc::ECONNRESET, "ECONNRESET"),
        (libc::EPIPE, "EPIPE"),
        (libc::ETIMEDOUT, "ETIMEDOUT"),
        (libc::EAGAIN, "EAGAIN"),
        (libc::ENOTDIR, "ENOTDIR"),
        (libc::ELOOP, "ELOOP"),
        (libc::ENAMETOOLONG, "ENAMETOOLONG"),
        (libc::EPROTOTYPE, "EPROTOTYPE"),
        (libc::EINVAL, "EINVAL"),
        (libc::EADDRNOTAVAIL, "EADDRNOTAVAIL"),
        (libc::ENOTCONN, "ENOTCONN"),
    ];
    names.iter().find(|(c, _)| *c == n).map(|(_, s)| s.to_string()).unwrap_or_else(|| "UNKNOWN".into())
}

/// Probe the desktop inbox over the IPC socket. Never fails: every outcome is
/// either a trustworthy answer or a stated reason it is not one.
pub async fn probe_inbox() -> Probe {
    let Some(sp) = socket_path() else { return Probe::default() };
    // No desktop socket at all is a legitimate "nothing to report": the canvas
    // is not running, so there is no inbox to miss.
    if !dir_is_safe(&sp.dir) || !std::path::Path::new(&sp.sock).exists() {
        return Probe::default();
    }
    match tokio::time::timeout(Duration::from_millis(TIMEOUT_MS), ask(&sp.sock)).await {
        Ok(p) => p,
        Err(_) => Probe::unknown(format!("timeout after {TIMEOUT_MS}ms")),
    }
}

async fn ask(sock: &str) -> Probe {
    let mut s = match tokio::net::UnixStream::connect(sock).await {
        Ok(s) => s,
        Err(e) => return Probe::unknown(format!("socket error: connect {} {}", errno_name(&e), sock)),
    };
    if let Err(e) = s.write_all(b"{\"op\":\"list_inbox\"}\n").await {
        return Probe::unknown(format!("socket error: write {}", errno_name(&e)));
    }
    let mut buf: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        match s.read(&mut chunk).await {
            // A close with no complete frame says nothing; the budget decides.
            Ok(0) => return std::future::pending::<Probe>().await,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                if let Some(idx) = buf.iter().position(|b| *b == b'\n') {
                    return read_frame(&String::from_utf8_lossy(&buf[..idx]));
                }
            }
            Err(e) => return Probe::unknown(format!("socket error: read {}", errno_name(&e))),
        }
    }
}

/// One reply frame, read the way the probe reads it.
pub fn read_frame(line: &str) -> Probe {
    let Some(frame) = js::parse(line) else { return Probe::unknown("unparseable frame") };
    let op = js::get(Some(&frame), "op");
    if matches!(op, Some(J::Str(s)) if s == "inbox") {
        let sessions = sanitize_inbox_rows(js::get(Some(&frame), "sessions"));
        return Probe { sessions, attn_dark: dark_count(js::get(Some(&frame), "attn_dark")), unknown: None };
    }
    // denied / no_session / no_session_stale / error: refusals, not emptiness.
    let name = if js::truthy(op) { js::to_string(op) } else { "unknown".into() };
    Probe::unknown(format!("desktop replied {name}"))
}

// ── right-checkout helpers ──────────────────────────────────────────────────

/// Reduce a git remote to `host/path`: no scheme, no credentials, no port, no
/// trailing `.git` or slash, lowercased. This mirrors the server's own
/// canonicaliser rule for rule and in the same order; a shared vector table on
/// both sides keeps them honest. Each forge rule is confined to where that
/// forge can be, because a fold we invent drains edits into the wrong repo.
pub fn normalize_remote(u: Option<&J>) -> Option<String> {
    let Some(J::Str(u)) = u else { return None };
    let mut v = js::trim(u).to_lowercase();
    if v.is_empty() {
        return None;
    }
    // scheme://
    if let Some(colon) = v.find(':') {
        let scheme = &v[..colon];
        let mut sc = scheme.chars();
        if sc.next().is_some_and(|c| c.is_ascii_lowercase())
            && sc.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "+.-".contains(c))
            && v[colon..].starts_with("://")
        {
            v = v[colon + 3..].to_string();
        }
    }
    // user[:password]@
    if let Some(i) = v.find(['/', '@']) {
        if i > 0 && v.as_bytes()[i] == b'@' {
            v = v[i + 1..].to_string();
        }
    }
    // host:port/ -> host/
    if let Some(i) = v.find(['/', ':']) {
        if i > 0 && v.as_bytes()[i] == b':' {
            let rest = &v[i + 1..];
            let digits = rest.bytes().take_while(|b| b.is_ascii_digit()).count();
            if digits > 0 && rest.as_bytes().get(digits) == Some(&b'/') {
                v = format!("{}/{}", &v[..i], &rest[digits + 1..]);
            }
        }
    }
    // scp-like host:path
    if let Some(i) = v.find(['/', ':']) {
        if i > 0 && v.as_bytes()[i] == b':' {
            v = format!("{}/{}", &v[..i], &v[i + 1..]);
        }
    }
    // collapse separators
    let mut collapsed = String::with_capacity(v.len());
    for c in v.chars() {
        if c == '/' && collapsed.ends_with('/') {
            continue;
        }
        collapsed.push(c);
    }
    v = collapsed;
    if let Some(s) = v.strip_suffix(".git") {
        v = s.to_string();
    }
    v = v.trim_end_matches('/').to_string();
    // Bitbucket Data Center, anchored by shape: <host>/scm/<PROJECT>/<repo>.
    {
        let segs: Vec<&str> = v.split('/').collect();
        if segs.len() == 4 && segs[1] == "scm" && segs.iter().all(|s| !s.is_empty()) {
            v = format!("{}/{}/{}", segs[0], segs[2], segs[3]);
        }
    }
    // Azure DevOps, anchored to its fixed hostnames.
    if let Some(rest) = v.strip_prefix("ssh.dev.azure.com/") {
        v = format!("dev.azure.com/{rest}");
    }
    if let Some(rest) = v.strip_prefix("dev.azure.com/v3/") {
        v = format!("dev.azure.com/{rest}");
    }
    if let Some(rest) = v.strip_prefix("dev.azure.com/") {
        let segs: Vec<&str> = rest.splitn(4, '/').collect();
        if segs.len() == 4 && !segs[0].is_empty() && !segs[1].is_empty() && segs[2] == "_git" {
            v = format!("dev.azure.com/{}/{}/{}", segs[0], segs[1], segs[3]);
        }
    }
    (!v.is_empty()).then_some(v)
}

/// `/^gitdir:\s*(.+)$/m` against a worktree's `.git` file.
fn gitdir_link(text: &str) -> Option<String> {
    let chars: Vec<char> = text.chars().collect();
    let lit: Vec<char> = "gitdir:".chars().collect();
    for start in 0..chars.len() {
        if start > 0 && !js::is_line_term(chars[start - 1]) {
            continue;
        }
        if chars.len() < start + lit.len() || chars[start..start + lit.len()] != lit[..] {
            continue;
        }
        let p0 = start + lit.len();
        let mut ws_end = p0;
        while ws_end < chars.len() && js::is_ws(chars[ws_end]) {
            ws_end += 1;
        }
        // Greedy whitespace, backing off until `.+` has a character to take.
        let p = (p0..=ws_end).rev().find(|&p| p < chars.len() && !js::is_line_term(chars[p]));
        if let Some(p) = p {
            let end = (p..chars.len()).find(|&i| js::is_line_term(chars[i])).unwrap_or(chars.len());
            return Some(chars[p..end].iter().collect());
        }
    }
    None
}

/// `/\[remote "origin"\][^[]*?url\s*=\s*([^\n]+)/` against a git config.
fn origin_url(cfg: &str) -> Option<String> {
    let chars: Vec<char> = cfg.chars().collect();
    let header: Vec<char> = "[remote \"origin\"]".chars().collect();
    let at = |i: usize, lit: &str| -> bool {
        let l: Vec<char> = lit.chars().collect();
        chars.len() >= i + l.len() && chars[i..i + l.len()] == l[..]
    };
    for h in 0..chars.len() {
        if chars.len() < h + header.len() || chars[h..h + header.len()] != header[..] {
            continue;
        }
        let mut i = h + header.len();
        loop {
            if at(i, "url") {
                let mut j = i + 3;
                while j < chars.len() && js::is_ws(chars[j]) {
                    j += 1;
                }
                if j < chars.len() && chars[j] == '=' {
                    let eq_end = j + 1;
                    let mut q = eq_end;
                    while q < chars.len() && js::is_ws(chars[q]) {
                        q += 1;
                    }
                    let p = (eq_end..=q).rev().find(|&p| p < chars.len() && chars[p] != '\n');
                    if let Some(p) = p {
                        let end = (p..chars.len()).find(|&k| chars[k] == '\n').unwrap_or(chars.len());
                        return Some(chars[p..end].iter().collect());
                    }
                }
            }
            if i >= chars.len() || chars[i] == '[' {
                break;
            }
            i += 1;
        }
    }
    None
}

/// The identity of the repo at `cwd`, normalised, or `None` with no git.
///
/// Origin first. With no origin the repo is local-only, and a local repo's
/// identity is its own (symlink-resolved) path, which is what the server
/// records for one. A linked worktree's `.git` is a file, and its remote lives
/// in the main repository's config.
pub fn cwd_git_remote(cwd: &str) -> Option<String> {
    let mut git_dir = js::join(&[cwd, ".git"]);
    if std::fs::metadata(&git_dir).ok()?.is_file() {
        let link = gitdir_link(&js::read_utf8(&git_dir).ok()?)?;
        let mut wt = js::trim(&link).to_string();
        if !js::is_absolute(&wt) {
            wt = js::resolve(&[cwd, &wt]);
        }
        git_dir = match js::read_utf8(&js::join(&[&wt, "commondir"])) {
            Ok(c) => {
                let common = js::trim(&c).to_string();
                if js::is_absolute(&common) {
                    common
                } else {
                    js::resolve(&[&wt, &common])
                }
            }
            Err(_) => wt,
        };
    }
    let cfg = js::read_utf8(&js::join(&[&git_dir, "config"])).ok()?;
    if let Some(url) = origin_url(&cfg) {
        return normalize_remote(Some(&J::Str(url)));
    }
    let root = std::fs::canonicalize(cwd)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| cwd.to_string());
    normalize_remote(Some(&J::Str(format!("file://{root}"))))
}

/// True when two remotes name the same repo.
pub fn remotes_match(a: Option<&J>, b: Option<&J>) -> bool {
    match (normalize_remote(a), normalize_remote(b)) {
        (Some(x), Some(y)) => x == y,
        _ => false,
    }
}

fn origin_j(origin: &Option<String>) -> Option<J> {
    origin.as_ref().map(|s| J::Str(s.clone()))
}

/// `decodeURI`: escapes for reserved characters stay escaped; a malformed
/// sequence is an error.
fn decode_uri(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(b.len());
    let mut i = 0;
    let hex = |c: u8| (c as char).to_digit(16).map(|d| d as u8);
    while i < b.len() {
        if b[i] != b'%' {
            out.push(b[i]);
            i += 1;
            continue;
        }
        let byte = |k: usize| -> Option<u8> {
            if b.get(k) != Some(&b'%') {
                return None;
            }
            Some(hex(*b.get(k + 1)?)? << 4 | hex(*b.get(k + 2)?)?)
        };
        let first = byte(i)?;
        if first < 0x80 {
            if ";/?:@&=+$,#".contains(first as char) {
                out.extend_from_slice(&b[i..i + 3]);
            } else {
                out.push(first);
            }
            i += 3;
            continue;
        }
        let n = match first {
            0xC2..=0xDF => 2,
            0xE0..=0xEF => 3,
            0xF0..=0xF4 => 4,
            _ => return None,
        };
        let mut seq = vec![first];
        for k in 1..n {
            seq.push(byte(i + 3 * k)?);
        }
        std::str::from_utf8(&seq).ok()?;
        out.extend_from_slice(&seq);
        i += 3 * n;
    }
    String::from_utf8(out).ok()
}

/// A `file://` remote as a filesystem path, or `None` for anything else.
pub fn local_checkout_path(remote: Option<&J>) -> Option<String> {
    let Some(J::Str(r)) = remote else { return None };
    let t = js::trim(r);
    if !js::starts_with_ci(t, "file://") {
        return None;
    }
    let rest = &t[7..];
    if rest.is_empty() || rest.chars().any(js::is_line_term) {
        return None;
    }
    Some(decode_uri(rest).unwrap_or_else(|| rest.to_string()))
}

/// True when `a` and `b` are the same directory, or one contains the other.
fn same_tree(a: &str, b: &str) -> bool {
    if a.is_empty() || b.is_empty() {
        return false;
    }
    let (ra, rb) = (js::resolve(&[a]), js::resolve(&[b]));
    if ra == rb {
        return true;
    }
    let inside = |root: &str, child: &str| {
        let rel = js::relative(root, child);
        !rel.is_empty() && !rel.starts_with("..") && !js::is_absolute(&rel)
    };
    inside(&ra, &rb) || inside(&rb, &ra)
}

/// The session's own checkout, when it is a real repo inside the tree the
/// person opened. A server-supplied path never sends an agent elsewhere.
pub fn reachable_checkout(s: &J, cwd: &str) -> Option<String> {
    let p = local_checkout_path(js::get(Some(s), "repo_remote"))?;
    if cwd.is_empty() || !same_tree(cwd, &p) {
        return None;
    }
    std::fs::metadata(js::join(&[&p, ".git"])).ok().map(|_| p)
}

/// Whether a page session is drainable from `cwd`.
pub fn page_drainable_here(s: &J, origin: &Option<String>, cwd: &str) -> bool {
    let remote = js::get(Some(s), "repo_remote");
    if !js::truthy(remote) {
        return true;
    }
    if remotes_match(origin_j(origin).as_ref(), remote) {
        return true;
    }
    reachable_checkout(s, cwd).is_some()
}

fn sum(rows: &[&J], key: &str) -> f64 {
    rows.iter().fold(0.0, |a, s| a + js::num_or_zero(js::get(Some(s), key)))
}

/// Insertion-ordered de-duplication, as a `Set` does it.
fn unique<T: PartialEq + Clone>(items: impl IntoIterator<Item = T>) -> Vec<T> {
    let mut out: Vec<T> = Vec::new();
    for x in items {
        if !out.contains(&x) {
            out.push(x);
        }
    }
    out
}

/// The safety branches the rows require, read from each row, never derived.
pub fn safety_branches(rows: &[&J]) -> Vec<String> {
    unique(rows.iter().filter_map(|s| match js::get(Some(s), "safety_branch") {
        b @ Some(J::Str(x)) if is_safe_branch_name(b) => Some(x.clone()),
        _ => None,
    }))
}

fn required_branch_hint(rows: &[&J]) -> String {
    let branches = safety_branches(rows);
    if branches.is_empty() {
        return String::new();
    }
    let label = if branches.len() > 1 { "Required safety branches" } else { "Required safety branch" };
    format!(" {label}: {} (server-owned; read from each row's safety_branch, do NOT derive).", branches.join(", "))
}

/// Single-quote a path an agent may act on. A path that reached here cannot
/// hold a quote (the remote guard refuses one), so this cannot be broken out of.
fn q(p: &str) -> String {
    format!("'{p}'")
}

fn checkout_path_hint(rows: &[&J], cwd: &str) -> String {
    let here = js::resolve(&[cwd]);
    let paths = unique(
        rows.iter()
            .filter_map(|s| reachable_checkout(s, cwd))
            .filter(|p| !p.is_empty() && js::resolve(&[p]) != here),
    );
    match paths.len() {
        0 => String::new(),
        1 => format!(" The checkout is {} - cd there before the branch checkout and the claim.", q(&paths[0])),
        _ => format!(
            " These live in separate checkouts under this folder: {} - run each claim from its own.",
            paths.iter().map(|p| q(p)).collect::<Vec<_>>().join(", ")
        ),
    }
}

/// Options for `summarize_inbox`.
#[derive(Clone, Copy, Debug)]
pub struct Opts {
    pub include_attention: bool,
    pub attn_dark: Option<f64>,
    /// A watcher is running for this session, so the next edit reaches the
    /// agent without it asking: the text says nothing about waiting.
    pub watcher_live: bool,
}

impl Default for Opts {
    fn default() -> Self {
        Opts { include_attention: true, attn_dark: None, watcher_live: false }
    }
}

/// How an agent with no watcher stays synced inside a long turn. One wait,
/// never a loop: every pass of a loop is a whole turn re-read, and a keepalive
/// every half minute made an idle stretch the most expensive part of a session.
pub const WAIT_ONCE: &str = "If your turn goes on beside the open canvas after applying them, wait once for the next edit \
(less_canvas_inbox with wait_seconds). Do not loop the wait: each pass costs a whole turn.";

/// The agent-facing wake text, routed by surface and checkout. Empty when
/// there is nothing actionable to say.
pub fn summarize_inbox(sessions: &[J], cwd: &str, opts: Opts) -> String {
    let origin = cwd_git_remote(cwd);
    let (mut here, mut elsewhere, mut artefact, mut annotations, mut attention, mut recoverable) =
        (vec![], vec![], vec![], vec![], vec![], vec![]);
    for s in sessions {
        let n = |k: &str| js::num_or_zero(js::get(Some(s), k));
        if n("n_page") > 0.0 {
            if page_drainable_here(s, &origin, cwd) {
                here.push(s)
            } else {
                elsewhere.push(s)
            }
        }
        if n("n_artefact") > 0.0 {
            artefact.push(s);
        }
        if n("n_annotation") > 0.0 {
            annotations.push(s);
        }
        if n("n_needs_human") > 0.0 {
            attention.push(s);
        }
        if js::truthy(js::get(Some(s), "recoverable")) {
            recoverable.push(s);
        }
    }
    let mut lines: Vec<String> = Vec::new();
    if !here.is_empty() {
        lines.push(format!(
            "{} page edit(s) are drainable from this checkout. These are Type-2 SOURCE ops - work BRANCH-FIRST: \
READ the required branch from the session's safety_branch field (on the less_canvas_inbox row, also on less_canvas_status), then \
git checkout -b <safety_branch> (or git checkout it if it already exists) BEFORE you claim - the server \
withholds every source op unless you are on that safety branch. If a session's safety_branch is null it is un-stamped: no branch is required. \
On EVERY source claim AND ack pass repo_branch (= git rev-parse --abbrev-ref HEAD) and checkout_head (= git rev-parse HEAD). \
Then apply with less_canvas_ops (claim -> apply each on previous_value, bottom-up per file -> ack), \
and let the canvas re-capture. Apply them now; do not ask first.{}{}",
            js::num_to_string(sum(&here, "n_page")),
            required_branch_hint(&here),
            checkout_path_hint(&here, cwd)
        ));
    }
    for s in &elsewhere {
        let r = js::get(Some(s), "repo_remote");
        let h = js::get(Some(s), "source_hint");
        let target = if js::truthy(r) {
            js::to_string(r)
        } else if js::truthy(h) {
            js::to_string(h)
        } else {
            "another repo".into()
        };
        lines.push(format!(
            "{} page edit(s) target {}; this session is rooted elsewhere - tell the user to run /designless from that repo (do NOT claim here).",
            js::num_to_string(js::num_or_zero(js::get(Some(s), "n_page"))),
            target
        ));
    }
    if !artefact.is_empty() {
        // The DOCUMENT leads the sentence; the agent names it, never the machinery.
        let docs: Vec<J> = unique(artefact.iter().map(|s| {
            for k in ["title", "brand_slug"] {
                let v = js::get(Some(s), k);
                if js::truthy(v) {
                    return v.cloned().unwrap();
                }
            }
            let r = js::get(Some(s), "repo_remote");
            if js::truthy(r) {
                let tail = js::to_string(r).rsplit('/').next().unwrap_or("").to_string();
                if !tail.is_empty() {
                    return J::Str(tail);
                }
            }
            J::Str("Untitled".into())
        }));
        lines.push(format!(
            "{} edit(s) are waiting on the Designless app for {}. Name the document, never the machinery, when you tell the user. Apply them now with less_canvas_ops action 'apply_type1' (one call claims, applies and acks; the manifest IS the source, so it applies server-side and needs NO checkout and no branch).",
            js::num_to_string(sum(&artefact, "n_artefact")),
            docs.iter().map(|d| format!("\"{}\"", js::to_string(Some(d)))).collect::<Vec<_>>().join(", ")
        ));
    }
    if !annotations.is_empty() {
        lines.push(format!(
            "{} annotation(s) are waiting. They are not mechanical edits: read them as context with less_canvas_ops action=peek, form your judgment, then ack them applied. Do this now; do not ask first.",
            js::num_to_string(sum(&annotations, "n_annotation"))
        ));
    }
    if !attention.is_empty() && opts.include_attention {
        // Inform-only: the item belongs to the user and is actionable in the canvas.
        let anchors: Vec<J> = unique(attention.iter().map(|s| {
            for k in ["brand_slug", "repo_remote"] {
                let v = js::get(Some(s), k);
                if js::truthy(v) {
                    return v.cloned().unwrap();
                }
            }
            J::Str("a canvas".into())
        }));
        lines.push(format!(
            "{} of the user's edit(s) are waiting for them in the canvas ({}) - \
the canvas shows this where the edit happened. Do not act on it and do not relay ids; \
if the user is present you may mention it once in their words. This do-not-act rule covers \
this line and the day-old line below it and NOTHING else: the waiting edits above are yours to apply.",
            js::num_to_string(sum(&attention, "n_needs_human")),
            anchors.iter().map(|a| js::to_string(Some(a))).collect::<Vec<_>>().join(", ")
        ));
    }
    // Fail-safe A: an item the canvas has not shown for more than a day. It can
    // stand alone, because its session may have left this listing.
    let dark = opts.attn_dark.filter(|d| *d > 0.0).unwrap_or(0.0);
    if dark > 0.0 && opts.include_attention {
        lines.push(format!(
            "{} of the user's edit(s) {} waited more than a day for their attention without being seen in the Designless app - \
tell the user plainly, once, that an edit of theirs is still waiting there. Do not act on it and do not relay ids.",
            js::num_to_string(dark),
            if dark == 1.0 { "has" } else { "have" }
        ));
    }
    if !recoverable.is_empty() {
        lines.push(format!(
            "{} expired session(s) still hold un-applied edits; they revive in place when you drain them (no work is lost).",
            recoverable.len()
        ));
    }
    // Only when there is something to drain, and only when no watcher will
    // bring the next edit: "after draining" over nothing is a heading over
    // empty space, and a wait beside a live watcher is paid for twice.
    if !opts.watcher_live && here.len() + elsewhere.len() + artefact.len() + annotations.len() + recoverable.len() > 0 {
        lines.push(WAIT_ONCE.into());
    }
    lines.join(" ")
}

/// Stable digest of the ATTENTION state, for the once-per-change gate.
pub fn attention_digest(sessions: &[J], attn_dark: Option<f64>) -> String {
    let mut rows: Vec<String> = sessions
        .iter()
        .filter(|s| js::num_or_zero(js::get(Some(s), "n_needs_human")) > 0.0)
        .map(|s| {
            let reason = js::get(Some(s), "attention_reason");
            format!(
                "{}:{}:{}",
                js::to_string(js::get(Some(s), "session_id")),
                js::to_string(js::get(Some(s), "n_needs_human")),
                if js::truthy(reason) { js::to_string(reason) } else { String::new() }
            )
        })
        .collect();
    rows.sort_by(|a, b| js::utf16_cmp(a, b));
    // Absent or zero, the dark count leaves the digest exactly as it was.
    if let Some(d) = attn_dark.filter(|d| *d > 0.0) {
        rows.push(format!("dark:{}", js::num_to_string(d)));
    }
    if rows.is_empty() {
        return "none".into();
    }
    rows.join("|")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn s(v: &str) -> J {
        J::Str(v.into())
    }
    fn row(pairs: Vec<(&str, J)>) -> J {
        js::obj(pairs)
    }
    fn m(a: &str, b: &str) -> bool {
        remotes_match(Some(&s(a)), Some(&s(b)))
    }
    fn tmp(prefix: &str) -> String {
        let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let d = std::env::temp_dir().join(format!("{prefix}{}-{n}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        std::fs::canonicalize(&d).unwrap().to_string_lossy().into_owned()
    }
    fn git(cwd: &str, args: &[&str]) {
        let ok = Command::new("git").args(args).current_dir(cwd).output().unwrap().status.success();
        assert!(ok, "git {args:?}");
    }
    fn mk_repo(dir: &str) -> String {
        std::fs::create_dir_all(dir).unwrap();
        git(dir, &["init", "-q"]);
        dir.to_string()
    }
    fn page_row(repo: &str) -> J {
        row(vec![("title", s("Skyway")), ("n_page", J::Num(1.0)), ("repo_remote", s(&format!("file://{repo}"))), ("safety_branch", s("designless/abc"))])
    }

    // ── where the desktop is ──
    #[test]
    fn socket_path_default_is_the_per_user_address() {
        let sp = socket_path_from(None, None).unwrap();
        assert!(sp.sock.ends_with("/ipc.sock"));
        assert_eq!(js::dirname(&sp.sock), sp.dir);
    }

    #[test]
    fn socket_path_explicit_is_used_verbatim() {
        assert_eq!(
            socket_path_from(Some("/tmp/designless-501/other.sock"), None),
            Some(SockPath { dir: "/tmp/designless-501".into(), sock: "/tmp/designless-501/other.sock".into() })
        );
    }

    #[test]
    fn socket_path_blank_override_is_no_override() {
        assert!(socket_path_from(Some("   "), None).unwrap().sock.ends_with("/ipc.sock"));
    }

    // ── the shared vector table ──
    const SAME: &[(&str, &str, &str)] = &[
        ("github ssh vs https", "git@github.com:designlesshq/designless-agent.git", "https://github.com/designlesshq/designless-agent.git"),
        ("github ssh:// vs https", "ssh://git@github.com/org/repo.git", "https://github.com/org/repo.git"),
        ("github .git optional", "https://github.com/org/repo", "https://github.com/org/repo.git"),
        ("github trailing slash", "https://github.com/org/repo/", "git@github.com:org/repo.git"),
        ("github embedded token", "https://x-access-token:ghp_abc@github.com/org/repo.git", "git@github.com:org/repo.git"),
        ("github case", "git@github.com:DesignlessHQ/Designless-Agent.git", "https://github.com/designlesshq/designless-agent"),
        ("enterprise host", "git@git.acme.internal:team/repo.git", "https://git.acme.internal/team/repo.git"),
        ("gitlab subgroups", "git@gitlab.com:group/sub/sub2/repo.git", "https://gitlab.com/group/sub/sub2/repo.git"),
        ("gitlab custom ssh port", "ssh://git@gitlab.acme.com:2222/group/repo.git", "https://gitlab.acme.com/group/repo.git"),
        ("bitbucket cloud", "git@bitbucket.org:team/repo.git", "https://user@bitbucket.org/team/repo.git"),
        ("bitbucket data center", "ssh://git@bitbucket.acme.com:7999/PROJ/repo.git", "https://bitbucket.acme.com/scm/PROJ/repo.git"),
        ("bitbucket personal project", "ssh://git@bitbucket.acme.com:7999/~john.doe/repo.git", "https://bitbucket.acme.com/scm/~john.doe/repo.git"),
        ("azure devops", "git@ssh.dev.azure.com:v3/org/project/repo", "https://dev.azure.com/org/project/_git/repo"),
        ("azure devops with org user", "git@ssh.dev.azure.com:v3/org/project/repo", "https://org@dev.azure.com/org/project/_git/repo"),
        ("trailing newline from git stdout", "git@github.com:org/repo.git\n", "https://github.com/org/repo"),
        ("windows CRLF", "https://github.com/org/repo.git\r\n", "git@github.com:org/repo.git"),
        ("leading tab", "\tgit@github.com:org/repo.git", "https://github.com/org/repo"),
        ("doubled separator", "git@host:/org/repo.git", "https://host/org/repo"),
    ];
    const DIFFERENT: &[(&str, &str, &str)] = &[
        ("different repo", "git@github.com:org/repo-a.git", "git@github.com:org/repo-b.git"),
        ("different owner", "git@github.com:org-a/repo.git", "git@github.com:org-b/repo.git"),
        ("different host, same path", "git@github.com:org/repo.git", "git@gitlab.com:org/repo.git"),
        ("enterprise vs public github", "git@git.acme.internal:org/repo.git", "git@github.com:org/repo.git"),
        ("subgroup is not its parent", "https://gitlab.com/group/repo.git", "https://gitlab.com/group/sub/repo.git"),
        ("azure different project", "https://dev.azure.com/org/proj-a/_git/repo", "https://dev.azure.com/org/proj-b/_git/repo"),
    ];
    const NOT_A_FORGE_PATH: &[(&str, &str, &str)] = &[
        ("a group named scm is not Bitbucket", "https://gitlab.com/scm/build", "https://gitlab.com/build"),
        ("a group named v3 is not Azure", "https://gitlab.com/v3/build", "https://gitlab.com/build"),
        ("a path segment _git is not Azure", "https://gitlab.com/team/_git/build", "https://gitlab.com/team/build"),
        ("scm on a deeper path is not Bitbucket", "https://host.com/scm/a/b/c", "https://host.com/a/b/c"),
    ];

    #[test]
    fn same_repo_spellings_match() {
        for (label, a, b) in SAME {
            assert!(m(a, b), "{label}: {a} != {b}");
        }
    }

    #[test]
    fn different_repos_stay_different() {
        for (label, a, b) in DIFFERENT {
            assert!(!m(a, b), "{label}: {a} == {b}");
        }
    }

    #[test]
    fn a_forge_rule_stays_on_its_own_forge() {
        for (label, a, b) in NOT_A_FORGE_PATH {
            assert!(!m(a, b), "{label}: {a} == {b}");
        }
    }

    #[test]
    fn an_unknown_remote_never_matches_anything() {
        assert!(!remotes_match(Some(&J::Null), Some(&s("git@github.com:org/repo.git"))));
        assert!(!m("", "git@github.com:org/repo.git"));
        assert!(!remotes_match(Some(&s("git@github.com:org/repo.git")), None));
        assert!(!m("   ", "git@github.com:org/repo.git"));
    }

    // ── the guard in front of the normaliser ──
    #[test]
    fn the_guard_is_never_narrower_than_the_normaliser() {
        for (_, a, b) in SAME.iter().chain(NOT_A_FORGE_PATH) {
            for r in [a, b] {
                assert!(is_safe_repo_remote(Some(&s(r))), "the normaliser folds it, the guard refused it: {r}");
            }
        }
    }

    #[test]
    fn a_local_checkout_is_surfaced_not_erased() {
        let r = row(vec![("title", s("Skyway")), ("n_page", J::Num(4.0)), ("repo_remote", s("file:///Users/someone/Projects/skyway/site")), ("safety_branch", s("designless/60c93584"))]);
        assert!(is_safe_repo_remote(js::get(Some(&r), "repo_remote")));
        let kept = sanitize_inbox_rows(Some(&J::Arr(vec![r])));
        assert_eq!(kept.len(), 1);
        assert_eq!(js::get(Some(&kept[0]), "n_page"), Some(&J::Num(4.0)));
    }

    #[test]
    fn ssh_scheme_survives() {
        assert!(is_safe_repo_remote(Some(&s("ssh://git@bitbucket.acme.com:7999/PROJ/repo.git"))));
        assert!(is_safe_repo_remote(Some(&s("ssh://git@ssh.dev.azure.com:v3/org/project/repo"))));
    }

    #[test]
    fn injection_is_still_refused() {
        for bad in [
            "https://github.com/o/r.git; rm -rf /",
            "file:///tmp/$(whoami)",
            "git@host:o/r`id`",
            "https://host/o/r && curl evil.sh",
            "https://host/o/r\nwhoami",
            "https://host/o/r'",
            "file:///tmp/a\tb",
            "",
            "   ",
        ] {
            assert!(!is_safe_repo_remote(Some(&s(bad))), "should have been refused: {bad:?}");
        }
        assert!(!is_safe_repo_remote(Some(&J::Null)));
        assert!(!is_safe_repo_remote(Some(&J::Num(42.0))));
    }

    #[test]
    fn a_folder_with_a_space_is_not_thrown_away() {
        let spaced = "file:///Users/someone/My Projects/skyway";
        assert!(is_safe_repo_remote(Some(&s(spaced))));
        let r = row(vec![("title", s("x")), ("n_page", J::Num(2.0)), ("repo_remote", s(spaced)), ("safety_branch", s("designless/abc"))]);
        assert_eq!(sanitize_inbox_rows(Some(&J::Arr(vec![r]))).len(), 1);
    }

    #[test]
    fn a_path_an_agent_is_told_to_cd_into_is_quoted() {
        let root = tmp("dl-quote-");
        let repo = mk_repo(&format!("{root}/my site"));
        let out = summarize_inbox(&[page_row(&repo)], &root, Opts::default());
        assert!(out.contains(&format!("'{repo}'")), "{out}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_malformed_remote_drops_its_row_an_absent_one_does_not() {
        let base = |extra: Vec<(&str, J)>| {
            let mut v = vec![("title", s("x")), ("n_page", J::Num(1.0)), ("safety_branch", s("designless/abc"))];
            for (k, x) in extra {
                v.retain(|(kk, _)| *kk != k);
                v.push((k, x));
            }
            J::Arr(vec![row(v)])
        };
        assert_eq!(sanitize_inbox_rows(Some(&base(vec![("repo_remote", s("https://h/o/r; id"))]))).len(), 0);
        assert_eq!(sanitize_inbox_rows(Some(&base(vec![("repo_remote", J::Null)]))).len(), 1);
        assert_eq!(sanitize_inbox_rows(Some(&base(vec![("safety_branch", s("main"))]))).len(), 0);
    }

    #[test]
    fn reads_origin_from_a_clone_and_a_linked_worktree() {
        let root = tmp("dl-gate-");
        let repo = format!("{root}/repo");
        let tree = format!("{root}/wt");
        std::fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        git(&repo, &["remote", "add", "origin", "git@github.com:designlesshq/designless-agent.git"]);
        git(&repo, &["config", "user.email", "gate@example.invalid"]);
        git(&repo, &["config", "user.name", "gate"]);
        std::fs::write(format!("{repo}/f"), "x").unwrap();
        git(&repo, &["add", "f"]);
        git(&repo, &["commit", "-qm", "seed"]);
        git(&repo, &["worktree", "add", "-q", &tree, "-b", "side"]);
        let expected = Some("github.com/designlesshq/designless-agent".to_string());
        assert_eq!(cwd_git_remote(&repo), expected);
        assert!(std::fs::metadata(format!("{tree}/.git")).unwrap().is_file());
        assert_eq!(cwd_git_remote(&tree), expected);
        assert!(remotes_match(cwd_git_remote(&tree).map(J::Str).as_ref(), Some(&s("https://github.com/designlesshq/designless-agent.git"))));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn no_git_is_unknown_rather_than_wrong() {
        let d = tmp("dl-nogit-");
        assert_eq!(cwd_git_remote(&d), None);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_repo_with_no_origin_identifies_itself_by_its_path() {
        let d = mk_repo(&tmp("dl-local-"));
        let id = cwd_git_remote(&d);
        assert!(id.is_some());
        assert!(remotes_match(id.map(J::Str).as_ref(), Some(&s(&format!("file://{d}")))));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn an_origin_still_wins_over_the_path_fallback() {
        let d = mk_repo(&tmp("dl-origin-"));
        git(&d, &["remote", "add", "origin", "git@github.com:org/repo.git"]);
        assert!(remotes_match(cwd_git_remote(&d).map(J::Str).as_ref(), Some(&s("https://github.com/org/repo"))));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn the_drain_gate_recognises_a_local_checkout_as_here() {
        let d = mk_repo(&tmp("dl-e2e-"));
        let r = row(vec![("title", s("Skyway")), ("n_page", J::Num(4.0)), ("repo_remote", s(&format!("file://{d}"))), ("safety_branch", s("designless/60c93584"))]);
        let origin = cwd_git_remote(&d);
        let here: Vec<J> = sanitize_inbox_rows(Some(&J::Arr(vec![r])))
            .into_iter()
            .filter(|x| js::num_or_zero(js::get(Some(x), "n_page")) > 0.0 && page_drainable_here(x, &origin, &d))
            .collect();
        assert_eq!(here.len(), 1);
        let _ = std::fs::remove_dir_all(&d);
    }

    // ── finding the checkout you are standing in ──
    #[test]
    fn local_checkout_path_reads_file_remotes_only() {
        assert_eq!(local_checkout_path(Some(&s("file:///Users/me/app"))).as_deref(), Some("/Users/me/app"));
        assert_eq!(local_checkout_path(Some(&s("file:///Users/me/my%20app"))).as_deref(), Some("/Users/me/my app"));
        assert_eq!(local_checkout_path(Some(&s("https://github.com/o/r.git"))), None);
        assert_eq!(local_checkout_path(Some(&s("git@github.com:o/r.git"))), None);
        assert_eq!(local_checkout_path(Some(&J::Null)), None);
        // decodeURI leaves reserved escapes alone and returns a malformed one raw.
        assert_eq!(local_checkout_path(Some(&s("file:///a%2Fb"))).as_deref(), Some("/a%2Fb"));
        assert_eq!(local_checkout_path(Some(&s("file:///a%zz"))).as_deref(), Some("/a%zz"));
    }

    #[test]
    fn the_repo_one_directory_below_is_here() {
        let root = tmp("dl-below-");
        let repo = mk_repo(&format!("{root}/site"));
        let r = page_row(&repo);
        assert!(page_drainable_here(&r, &cwd_git_remote(&root), &root));
        assert_eq!(reachable_checkout(&r, &root), Some(repo));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn standing_inside_the_repo_is_here() {
        let repo = mk_repo(&tmp("dl-inside-"));
        let sub = format!("{repo}/src/deep");
        std::fs::create_dir_all(&sub).unwrap();
        assert!(page_drainable_here(&page_row(&repo), &cwd_git_remote(&sub), &sub));
        let _ = std::fs::remove_dir_all(&repo);
    }

    #[test]
    fn a_checkout_outside_the_opened_tree_is_not_here() {
        let a = tmp("dl-a-");
        let b = mk_repo(&tmp("dl-b-"));
        assert!(!page_drainable_here(&page_row(&b), &cwd_git_remote(&a), &a));
        assert_eq!(reachable_checkout(&page_row(&b), &a), None);
        let _ = std::fs::remove_dir_all(&a);
        let _ = std::fs::remove_dir_all(&b);
    }

    #[test]
    fn a_path_in_the_tree_that_is_not_a_repo_is_not_here() {
        let root = tmp("dl-norepo-");
        let not_repo = format!("{root}/site");
        std::fs::create_dir_all(&not_repo).unwrap();
        assert!(!page_drainable_here(&page_row(&not_repo), &cwd_git_remote(&root), &root));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_drain_line_names_the_checkout_when_elsewhere_in_the_tree() {
        let root = tmp("dl-hint-");
        let repo = mk_repo(&format!("{root}/site"));
        let out = summarize_inbox(&[page_row(&repo)], &root, Opts::default());
        assert!(out.contains("drainable from this checkout"));
        assert!(out.contains(&repo), "{out}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_remote_backed_session_is_unaffected() {
        let repo = mk_repo(&tmp("dl-remote-"));
        git(&repo, &["remote", "add", "origin", "git@github.com:org/repo.git"]);
        let r = row(vec![("title", s("x")), ("n_page", J::Num(1.0)), ("repo_remote", s("https://github.com/org/repo")), ("safety_branch", s("designless/abc"))]);
        assert!(page_drainable_here(&r, &cwd_git_remote(&repo), &repo));
        let other = row(vec![("title", s("x")), ("n_page", J::Num(1.0)), ("repo_remote", s("https://github.com/org/DIFFERENT")), ("safety_branch", s("designless/abc"))]);
        assert!(!page_drainable_here(&other, &cwd_git_remote(&repo), &repo));
        let _ = std::fs::remove_dir_all(&repo);
    }

    // ── the dark count ──
    #[test]
    fn dark_count_absence_is_unknown_never_zero() {
        assert_eq!(dark_count(Some(&J::Num(1.0))), Some(1.0));
        assert_eq!(dark_count(Some(&J::Num(0.0))), Some(0.0));
        assert_eq!(dark_count(Some(&s("2"))), Some(2.0));
        assert_eq!(dark_count(None), None);
        assert_eq!(dark_count(Some(&J::Null)), None);
        assert_eq!(dark_count(Some(&s("many"))), None);
        assert_eq!(dark_count(Some(&J::Num(-1.0))), None);
    }

    #[test]
    fn attention_digest_without_dark_is_unchanged() {
        let rows = vec![
            row(vec![("session_id", s("b")), ("n_needs_human", J::Num(1.0)), ("attention_reason", s("gate_refused"))]),
            row(vec![("session_id", s("a")), ("n_needs_human", J::Num(2.0))]),
        ];
        assert_eq!(attention_digest(&rows, None), "a:2:|b:1:gate_refused");
        assert_eq!(attention_digest(&rows, Some(0.0)), "a:2:|b:1:gate_refused");
        assert_eq!(attention_digest(&[], None), "none");
    }

    #[test]
    fn attention_digest_moves_with_the_dark_count() {
        assert_eq!(attention_digest(&[], Some(1.0)), "dark:1");
        assert_ne!(attention_digest(&[], Some(1.0)), attention_digest(&[], Some(2.0)));
        let rows = vec![row(vec![("session_id", s("a")), ("n_needs_human", J::Num(1.0))])];
        assert_ne!(attention_digest(&rows, Some(1.0)), attention_digest(&rows, None));
    }

    fn tmpdir() -> String {
        std::env::temp_dir().to_string_lossy().into_owned()
    }

    #[test]
    fn a_dark_count_speaks_beside_an_empty_listing_inform_only() {
        let text = summarize_inbox(&[], &tmpdir(), Opts { include_attention: true, attn_dark: Some(1.0), ..Opts::default() });
        assert!(text.contains("waited more than a day"));
        assert!(text.contains("Designless app"));
        assert!(text.contains("Do not act on it"));
        assert!(!text.contains("After applying"));
        for w in ["session_id", "claim", "apply_type1", "Apply them on sight"] {
            assert!(!text.contains(w), "{w}");
        }
    }

    #[test]
    fn the_dark_line_obeys_the_gate() {
        assert_eq!(summarize_inbox(&[], &tmpdir(), Opts { include_attention: false, attn_dark: Some(3.0), ..Opts::default() }), "");
    }

    #[test]
    fn an_absent_dark_count_says_nothing() {
        for d in [None, Some(0.0)] {
            assert_eq!(summarize_inbox(&[], &tmpdir(), Opts { include_attention: true, attn_dark: d, ..Opts::default() }), "");
        }
    }

    #[test]
    fn the_apply_tail_follows_waiting_edits() {
        let rows = vec![row(vec![("session_id", s("s")), ("n_artefact", J::Num(1.0)), ("title", s("Deck"))])];
        let text = summarize_inbox(&rows, &tmpdir(), Opts { include_attention: true, attn_dark: Some(1.0), ..Opts::default() });
        assert!(text.contains(WAIT_ONCE));
        assert!(text.contains("waited more than a day"));
    }

    #[test]
    fn the_wait_is_one_wait_and_never_beside_a_live_watcher() {
        let rows = vec![row(vec![("session_id", s("s")), ("n_artefact", J::Num(1.0)), ("title", s("Deck"))])];
        let alone = summarize_inbox(&rows, &tmpdir(), Opts::default());
        assert!(alone.ends_with(WAIT_ONCE), "{alone}");
        assert!(!alone.contains("less_stream"), "a stream loop is a turn per keepalive");
        assert!(WAIT_ONCE.contains("Do not loop"));
        assert!(!WAIT_ONCE.contains('\u{2014}'));
        let watched = summarize_inbox(&rows, &tmpdir(), Opts { watcher_live: true, ..Opts::default() });
        assert!(watched.contains("\"Deck\""));
        for w in ["wait_seconds", "less_stream", "loop"] {
            assert!(!watched.contains(w), "{w}: {watched}");
        }
    }

    #[test]
    fn waiting_edits_end_on_the_instruction() {
        let rows = vec![row(vec![("session_id", s("s")), ("n_artefact", J::Num(2.0)), ("title", s("Making the Case for Goa"))])];
        let text = summarize_inbox(&rows, &tmpdir(), Opts::default());
        assert!(text.contains("\"Making the Case for Goa\""));
        assert!(text.contains("Apply them now"));
        let lower = text.to_lowercase();
        for w in ["if you want", "whenever you", "would you like"] {
            assert!(!lower.contains(w), "{w}");
        }
    }

    #[test]
    fn the_inform_only_rule_says_it_does_not_reach_the_edits_above() {
        let rows = vec![
            row(vec![("session_id", s("s")), ("n_artefact", J::Num(1.0)), ("title", s("Deck"))]),
            row(vec![("session_id", s("t")), ("n_needs_human", J::Num(1.0)), ("brand_slug", s("acme"))]),
        ];
        let text = summarize_inbox(&rows, &tmpdir(), Opts::default());
        assert!(text.contains("Do not act on it"));
        assert!(text.contains("the waiting edits above are yours to apply"));
    }

    #[test]
    fn an_attention_only_message_has_no_apply_tail() {
        let rows = vec![row(vec![("session_id", s("s")), ("n_needs_human", J::Num(1.0)), ("brand_slug", s("acme"))])];
        let text = summarize_inbox(&rows, &tmpdir(), Opts::default());
        assert!(text.contains("waiting for them in the canvas"));
        assert!(!text.contains("After applying"));
    }

    #[tokio::test]
    async fn no_desktop_socket_is_emptiness_never_an_unknown() {
        // The env var is read in-process; this is the only test in the crate
        // that sets it, so it cannot race another reader.
        std::env::set_var("DESIGNLESS_IPC_SOCKET", "/tmp/designless-a-socket-that-is-not-there/ipc.sock");
        let r = probe_inbox().await;
        std::env::remove_var("DESIGNLESS_IPC_SOCKET");
        assert_eq!(r.unknown, None);
        assert_eq!(r.count(), 0);
    }

    #[test]
    fn a_reply_frame_is_read_as_the_probe_reads_it() {
        let p = read_frame(r#"{"op":"inbox","sessions":[{"session_id":"a","n_artefact":1},{"repo_remote":"x; y"}],"attn_dark":2}"#);
        assert_eq!((p.count(), p.attn_dark, p.unknown.clone()), (1, Some(2.0), None));
        assert_eq!(read_frame(r#"{"op":"no_session_stale"}"#).unknown.as_deref(), Some("desktop replied no_session_stale"));
        assert_eq!(read_frame("null").unknown.as_deref(), Some("desktop replied unknown"));
        assert_eq!(read_frame("{nope").unknown.as_deref(), Some("unparseable frame"));
    }
}
