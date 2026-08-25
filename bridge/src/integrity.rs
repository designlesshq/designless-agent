//! Plugin tree attestation — the client half of the integrity fence.
//!
//! Every upstream frame carries `x-designless-plugin-integrity: v=<ver>;h=<hex>`,
//! where the hash covers the plugin's behavior-bearing tree. The server owns
//! the expected value (published per release) and refuses at `initialize` on
//! mismatch, so a locally modified plugin never gets a tool surface at all.
//!
//! The hash is recomputed for EVERY frame, not once at startup: sessions run
//! for hours, and the fence that matters is "was the tree intact when the
//! agent last acted", which per-frame attestation answers exactly — any edit
//! is caught on the next call that crosses the bridge.
//!
//! Honesty note carried from the design: this is tamper-EVIDENT, not
//! tamper-proof. A local user owns these files and this process. What the
//! fence guarantees is that a drifted tree cannot reach the server's
//! capabilities and that the drift is named to the user with a one-command
//! recovery. `DESIGNLESS_BRIDGE_DEV=1` sends `h=dev` — an honest opt-out the
//! server sees and marks; it exists because maintainers edit trees for a
//! living.
//!
//! The same algorithm lives in `bin/tree-hash.mjs` (used by the release
//! tooling to publish the expected hash). `release.mjs` cross-checks the two
//! implementations against the real tree via `--integrity` before every
//! release, so drift between them cannot ship.

use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Directories whose entire contents are attested (behavior-bearing).
/// Additions inside these directories change the hash — a planted skill or
/// hook is a violation, not noise.
///
/// These are the directories WE own. The host-owned namespaces (`.agents`,
/// `.claude-plugin`, `.codex-plugin`, `.cursor-plugin`) were here once and
/// are deliberately gone: a host writes its own files into its own namespace
/// at install time, and attesting a directory we do not own turns the host's
/// routine work into what looks like tampering.
///
/// Codex does exactly that. It migrates our `commands/agent.md` into
/// `.codex-plugin/migrated-command-skills/source-command-agent/SKILL.md`,
/// wrapping our content in ITS template — so the bytes depend on the Codex
/// version, not ours, and can never be pinned by a release of this repo.
/// Every Codex install therefore hashed to a value no release could have
/// registered, and every Codex session was refused before auth.
///
/// The files we actually ship inside those namespaces are attested by exact
/// name in INCLUDE_FILES, so the manifests stay covered while the host's
/// additions are ignored. Rule of thumb: attest what we ship, not the
/// directory the host owns.
const INCLUDE_DIRS: &[&str] = &[
    "agents",
    "bin",
    "capsules",
    "commands",
    "docs",
    "hooks",
    "skills",
];

/// Individual files that are attested, by exact relative path — the root
/// files plus every manifest we ship inside a host-owned namespace.
///
/// An allow-list, so anything not named here and not inside an INCLUDE_DIR is
/// ignored. That is the counterintuitive half and it is deliberate: a file the
/// HOST adds is noise, while an edit to a manifest we ship is still caught.
/// `.github/checks/check-integrity-lists.mjs` fails the build if a file
/// appears in one of these namespaces without being listed here, so the
/// allow-list cannot silently stop covering something.
const INCLUDE_FILES: &[&str] = &[
    ".agents/plugins/marketplace.json",
    ".claude-plugin/marketplace.json",
    ".claude-plugin/plugin.json",
    ".codex-plugin/plugin.json",
    ".cursor-plugin/marketplace.json",
    ".cursor-plugin/plugin.json",
    ".gitignore",
    ".mcp.json",
    ".mcp.codex.json",
    ".mcp.cursor.json",
    "LICENSE",
    "README.md",
    "THIRD-PARTY-LICENSES.md",
    "llms.txt",
];

/// The one legitimately machine-varying file: the Cursor installer stamps
/// absolute paths into it. Normalizing the install root back to "." makes the
/// stamped and template forms hash identically.
const STAMPED_FILE: &str = ".mcp.cursor.json";

pub struct Integrity {
    root: PathBuf,
    version: String,
}

impl Integrity {
    /// Locate the plugin root from the running executable
    /// (`<root>/bin/designless-mcp-bridge-darwin-arm64`) and read the plugin
    /// version from the Claude manifest (identical across all manifests by
    /// the release lockstep).
    pub fn detect() -> Option<Self> {
        let exe = std::env::current_exe().ok()?;
        let root = exe.parent()?.parent()?.to_path_buf();
        let manifest = std::fs::read_to_string(root.join(".claude-plugin/plugin.json")).ok()?;
        let version = serde_json::from_str::<serde_json::Value>(&manifest)
            .ok()?
            .get("version")?
            .as_str()?
            .to_string();
        Some(Self { root, version })
    }

    /// The header value for one frame: `v=<version>;h=<hex|dev|error>`.
    /// `dev` is the maintainer opt-out; `error` means the tree could not be
    /// read at all (the server treats it as a mismatch).
    pub fn header_value(&self) -> String {
        if std::env::var("DESIGNLESS_BRIDGE_DEV").as_deref() == Ok("1") {
            return format!("v={};h=dev", self.version);
        }
        match tree_hash(&self.root) {
            Ok(h) => format!("v={};h={}", self.version, h),
            Err(e) => {
                tracing::warn!(error = %e, "integrity walk failed");
                format!("v={};h=error", self.version)
            }
        }
    }
}

/// Deterministic root hash of the attested tree:
/// sha256 over `relpath \0 hex(sha256(content)) \n` for every attested file,
/// sorted by relative path (byte order, `/` separators).
pub fn tree_hash(root: &Path) -> std::io::Result<String> {
    let mut entries: Vec<(String, String)> = Vec::new();

    for dir in INCLUDE_DIRS {
        let base = root.join(dir);
        if base.is_dir() {
            walk(root, &base, &mut entries)?;
        }
    }
    for file in INCLUDE_FILES {
        let path = root.join(file);
        if path.is_file() {
            entries.push((
                (*file).to_string(),
                file_hash(root, &path)?,
            ));
        }
    }

    entries.sort_by(|a, b| a.0.cmp(&b.0));

    let mut hasher = Sha256::new();
    for (rel, hash) in &entries {
        hasher.update(rel.as_bytes());
        hasher.update([0u8]);
        hasher.update(hash.as_bytes());
        hasher.update(*b"\n");
    }
    Ok(hex(&hasher.finalize()))
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, String)>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        if name == ".DS_Store" {
            continue; // macOS Finder litter, not a modification
        }
        let meta = std::fs::symlink_metadata(&path)?;
        let rel = path
            .strip_prefix(root)
            .map_err(std::io::Error::other)?
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        if meta.is_symlink() {
            // A symlink in the attested tree is itself a modification signal:
            // hash its target string so it participates deterministically.
            let target = std::fs::read_link(&path)?;
            let mut h = Sha256::new();
            h.update(b"symlink:");
            h.update(target.to_string_lossy().as_bytes());
            out.push((rel, hex(&h.finalize())));
        } else if meta.is_dir() {
            walk(root, &path, out)?;
        } else {
            out.push((rel.clone(), file_hash(root, &path)?));
        }
    }
    Ok(())
}

fn file_hash(root: &Path, path: &Path) -> std::io::Result<String> {
    let bytes = std::fs::read(path)?;
    let is_stamped = path
        .strip_prefix(root)
        .map(|r| r.to_string_lossy() == STAMPED_FILE)
        .unwrap_or(false);
    let mut hasher = Sha256::new();
    if is_stamped {
        // Replace every occurrence of the absolute install root with "." so
        // the Cursor-stamped form hashes identically to the shipped template.
        let text = String::from_utf8_lossy(&bytes);
        let root_str = root.to_string_lossy();
        let normalized = text.replace(root_str.as_ref(), ".");
        hasher.update(normalized.as_bytes());
    } else {
        hasher.update(&bytes);
    }
    Ok(hex(&hasher.finalize()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "dl-integrity-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("skills/orchestrator")).unwrap();
        fs::create_dir_all(dir.join("bin")).unwrap();
        fs::write(dir.join("skills/orchestrator/SKILL.md"), "content").unwrap();
        fs::write(dir.join("bin/launch.mjs"), "launcher").unwrap();
        fs::write(dir.join("README.md"), "readme").unwrap();
        fs::create_dir_all(dir.join(".claude-plugin")).unwrap();
        fs::write(dir.join(".claude-plugin/plugin.json"), r#"{"version":"1.0.0"}"#).unwrap();
        dir
    }

    #[test]
    fn hash_is_deterministic() {
        let dir = scratch();
        assert_eq!(tree_hash(&dir).unwrap(), tree_hash(&dir).unwrap());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn single_letter_edit_changes_hash() {
        let dir = scratch();
        let before = tree_hash(&dir).unwrap();
        fs::write(dir.join("skills/orchestrator/SKILL.md"), "cOntent").unwrap();
        assert_ne!(before, tree_hash(&dir).unwrap());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn added_file_in_behavior_dir_changes_hash() {
        let dir = scratch();
        let before = tree_hash(&dir).unwrap();
        fs::write(dir.join("skills/planted.md"), "surprise").unwrap();
        assert_ne!(before, tree_hash(&dir).unwrap());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn ds_store_and_unlisted_root_files_are_ignored() {
        let dir = scratch();
        let before = tree_hash(&dir).unwrap();
        fs::write(dir.join("skills/.DS_Store"), "finder").unwrap();
        fs::write(dir.join("CLAUDE.md"), "dev-only local file").unwrap();
        assert_eq!(before, tree_hash(&dir).unwrap());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_the_host_writes_into_its_own_namespace_is_ignored() {
        // The Codex case, reduced. Codex generates a skill under
        // `.codex-plugin/migrated-command-skills/` at install time, formatted by
        // its own template — content we neither ship nor control. While that
        // namespace was attested wholesale, every Codex install hashed to a
        // value no release could register and every session was refused.
        let dir = scratch();
        let before = tree_hash(&dir).unwrap();
        fs::create_dir_all(dir.join(".codex-plugin/migrated-command-skills/source-command-agent")).unwrap();
        fs::write(
            dir.join(".codex-plugin/migrated-command-skills/source-command-agent/SKILL.md"),
            "generated by the host, not by us",
        )
        .unwrap();
        assert_eq!(before, tree_hash(&dir).unwrap());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_manifest_we_ship_is_still_attested_inside_that_namespace() {
        // The other half, and the reason this is an allow-list rather than a
        // blanket exemption: ignoring the DIRECTORY must not mean ignoring the
        // files we put in it. Editing a shipped manifest is still tampering.
        let dir = scratch();
        let before = tree_hash(&dir).unwrap();
        fs::write(dir.join(".claude-plugin/plugin.json"), r#"{"version":"9.9.9"}"#).unwrap();
        assert_ne!(before, tree_hash(&dir).unwrap());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn cursor_stamp_normalizes_to_template() {
        let dir = scratch();
        let template = r#"{"args":["./bin/launch.mjs"],"cwd":"."}"#;
        fs::write(dir.join(".mcp.cursor.json"), template).unwrap();
        let template_hash = tree_hash(&dir).unwrap();
        let stamped = format!(
            r#"{{"args":["{r}/bin/launch.mjs"],"cwd":"{r}"}}"#,
            r = dir.to_string_lossy()
        );
        fs::write(dir.join(".mcp.cursor.json"), stamped).unwrap();
        assert_eq!(template_hash, tree_hash(&dir).unwrap());
        let _ = fs::remove_dir_all(&dir);
    }
}
