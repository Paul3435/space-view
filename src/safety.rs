//! Delete safety: which paths may be deleted, and with how much ceremony.
//!
//! This is pure string logic over Windows-style paths so it can be unit
//! tested on any platform. Comparison is case-insensitive and ignores `\\?\`
//! prefixes, forward slashes and trailing separators.

use crate::tree::NodeKind;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Protection {
    /// Ordinary item: one confirmation (two for permanent delete).
    Normal,
    /// System or profile location: the user must also type the item's name.
    Caution(String),
    /// Never deleted from disktree.
    Blocked(String),
}

/// Well-known system locations, normally read from the environment.
#[derive(Clone, Debug, Default)]
pub struct SystemPaths {
    pub windows: Option<String>,
    pub program_files: Vec<String>,
    pub program_data: Option<String>,
    pub users: Option<String>,
    pub profile: Option<String>,
}

impl SystemPaths {
    pub fn from_env() -> SystemPaths {
        let var = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
        let system_drive = var("SystemDrive").unwrap_or_else(|| "C:".to_owned());
        let windows = var("SystemRoot").or_else(|| var("windir")).or_else(|| Some(format!("{system_drive}\\Windows")));
        let mut program_files: Vec<String> = ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"]
            .iter()
            .filter_map(|k| var(k))
            .collect();
        if program_files.is_empty() {
            program_files.push(format!("{system_drive}\\Program Files"));
            program_files.push(format!("{system_drive}\\Program Files (x86)"));
        }
        let profile = var("USERPROFILE");
        let users = profile
            .as_deref()
            .and_then(|p| parent_of(&normalize(p)))
            .or_else(|| Some(normalize(&format!("{system_drive}\\Users"))));
        SystemPaths {
            windows,
            program_files,
            program_data: var("ProgramData").or_else(|| Some(format!("{system_drive}\\ProgramData"))),
            users,
            profile,
        }
    }
}

/// Lower-cased, backslash-separated, without `\\?\` prefix or trailing `\`.
pub fn normalize(path: &str) -> String {
    let mut s = path.replace('/', "\\");
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        s = format!(r"\\{rest}");
    } else if let Some(rest) = s.strip_prefix(r"\\?\") {
        s = rest.to_owned();
    }
    while s.len() > 2 && s.ends_with('\\') {
        s.pop();
    }
    s.to_lowercase()
}

fn parent_of(norm: &str) -> Option<String> {
    let idx = norm.rfind('\\')?;
    if idx == 0 {
        return None;
    }
    Some(norm[..idx].to_owned())
}

fn file_name(norm: &str) -> &str {
    norm.rsplit('\\').next().unwrap_or(norm)
}

/// `c:` or `\\server\share`.
pub fn is_volume_root(norm: &str) -> bool {
    let b = norm.as_bytes();
    if b.len() == 2 && b[1] == b':' && b[0].is_ascii_alphabetic() {
        return true;
    }
    if let Some(rest) = norm.strip_prefix(r"\\") {
        return rest.split('\\').filter(|c| !c.is_empty()).count() <= 2;
    }
    norm.is_empty() || norm == "\\" || norm == "/"
}

/// `inner` equals `outer` or lies inside it.
fn is_within(inner: &str, outer: &str) -> bool {
    inner == outer || (inner.starts_with(outer) && inner.as_bytes().get(outer.len()) == Some(&b'\\'))
}

const ROOT_BLOCKED: &[&str] = &[
    "$recycle.bin",
    "system volume information",
    "recovery",
    "boot",
    "efi",
    "$windows.~bt",
    "$windows.~ws",
    "$winreagent",
    "$sysreset",
    "config.msi",
    "pagefile.sys",
    "hiberfil.sys",
    "swapfile.sys",
    "dumpstack.log.tmp",
    "bootmgr",
    "bootnxt",
];

const PROFILE_KNOWN_FOLDERS: &[&str] = &[
    "appdata",
    "desktop",
    "documents",
    "downloads",
    "pictures",
    "music",
    "videos",
    "onedrive",
    "favorites",
    "contacts",
    "links",
    "saved games",
    "searches",
];

pub fn classify(path: &str, kind: NodeKind, sys: &SystemPaths) -> Protection {
    let p = normalize(path);
    if is_volume_root(&p) {
        return Protection::Blocked("This is the root of a drive.".to_owned());
    }
    if kind == NodeKind::Link {
        return Protection::Blocked(
            "This is a junction, symbolic link or mount point. It takes no space itself, and \
             disktree never deletes links because shell tools may treat the target as content."
                .to_owned(),
        );
    }
    let name = file_name(&p);
    if parent_of(&p).is_some_and(|parent| is_volume_root(&parent)) && ROOT_BLOCKED.contains(&name) {
        return Protection::Blocked(format!("\"{name}\" is used by Windows itself."));
    }
    if ["system volume information", "$recycle.bin"]
        .iter()
        .any(|n| p.split('\\').any(|c| c == *n))
    {
        return Protection::Blocked("This is inside a Windows-managed system folder.".to_owned());
    }

    let mut critical: Vec<(String, &str)> = Vec::new();
    if let Some(w) = &sys.windows {
        critical.push((normalize(w), "the Windows folder"));
    }
    for pf in &sys.program_files {
        critical.push((normalize(pf), "a Program Files folder"));
    }
    if let Some(pd) = &sys.program_data {
        critical.push((normalize(pd), "ProgramData"));
    }
    if let Some(u) = &sys.users {
        critical.push((normalize(u), "the Users folder"));
    }
    for (dir, label) in &critical {
        // Deleting a protected folder or anything that contains one.
        if is_within(dir, &p) {
            return Protection::Blocked(format!("This is {label} (or contains it)."));
        }
    }
    if let Some(profile) = &sys.profile {
        if is_within(&normalize(profile), &p) {
            return Protection::Blocked("This is your user profile folder (or contains it).".to_owned());
        }
    }
    for (dir, label) in &critical {
        if is_within(&p, dir) {
            if label == &"the Users folder" {
                return users_caution(&p, dir);
            }
            return Protection::Caution(format!(
                "This is inside {label}. Deleting it can break Windows or installed programs."
            ));
        }
    }
    if let Some(profile) = &sys.profile {
        let prof = normalize(profile);
        if parent_of(&p).as_deref() == Some(prof.as_str()) && PROFILE_KNOWN_FOLDERS.contains(&name) {
            return Protection::Caution(format!("This is your \"{name}\" folder."));
        }
    }
    Protection::Normal
}

fn users_caution(p: &str, users: &str) -> Protection {
    let rel = &p[users.len()..];
    let depth = rel.split('\\').filter(|c| !c.is_empty()).count();
    let comps: Vec<&str> = rel.split('\\').filter(|c| !c.is_empty()).collect();
    if depth == 1 {
        return Protection::Caution("This is a user's entire profile folder.".to_owned());
    }
    if depth == 2 && PROFILE_KNOWN_FOLDERS.contains(&comps[1]) {
        return Protection::Caution(format!("This is a user's \"{}\" folder.", comps[1]));
    }
    Protection::Normal
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sys() -> SystemPaths {
        SystemPaths {
            windows: Some(r"C:\WINDOWS".to_owned()),
            program_files: vec![r"C:\Program Files".to_owned(), r"C:\Program Files (x86)".to_owned()],
            program_data: Some(r"C:\ProgramData".to_owned()),
            users: Some(r"C:\Users".to_owned()),
            profile: Some(r"C:\Users\davap".to_owned()),
        }
    }

    fn c(p: &str) -> Protection {
        classify(p, NodeKind::Dir, &sys())
    }

    fn blocked(p: &str) -> bool {
        matches!(c(p), Protection::Blocked(_))
    }

    fn caution(p: &str) -> bool {
        matches!(c(p), Protection::Caution(_))
    }

    #[test]
    fn normalizes_paths() {
        assert_eq!(normalize(r"\\?\C:\Windows\"), r"c:\windows");
        assert_eq!(normalize("C:/Users/X/"), r"c:\users\x");
        assert_eq!(normalize(r"C:\"), "c:");
        assert_eq!(normalize(r"\\?\UNC\srv\share\x"), r"\\srv\share\x");
    }

    #[test]
    fn drive_and_share_roots_are_blocked() {
        assert!(blocked(r"C:\"));
        assert!(blocked(r"d:"));
        assert!(blocked(r"\\?\E:\"));
        assert!(blocked(r"\\nas\media"));
        assert!(!blocked(r"\\nas\media\old"));
        assert!(!blocked(r"D:\Games"));
    }

    #[test]
    fn system_folders_and_their_ancestors_are_blocked() {
        assert!(blocked(r"C:\Windows"));
        assert!(blocked(r"c:\windows\"));
        assert!(blocked(r"C:\Program Files"));
        assert!(blocked(r"C:\Program Files (x86)"));
        assert!(blocked(r"C:\ProgramData"));
        assert!(blocked(r"C:\Users"));
        assert!(blocked(r"C:\Users\davap"));
        assert!(blocked(r"C:\System Volume Information"));
        assert!(blocked(r"C:\$Recycle.Bin"));
        assert!(blocked(r"C:\$Recycle.Bin\S-1-5-21\x"));
        assert!(blocked(r"C:\pagefile.sys"));
        assert!(blocked(r"C:\Recovery"));
        // Not an ancestor, just a similar prefix.
        assert!(!blocked(r"C:\Windows.old"));
        assert!(!blocked(r"C:\Program Files Backup"));
    }

    #[test]
    fn inside_system_folders_needs_typed_confirmation() {
        assert!(caution(r"C:\Windows\System32"));
        assert!(caution(r"C:\Windows\Temp\big.log"));
        assert!(caution(r"C:\Program Files\Some App"));
        assert!(caution(r"C:\ProgramData\Package Cache"));
        assert!(caution(r"C:\Users\someoneelse"));
        assert!(caution(r"C:\Users\davap\Documents"));
        assert!(caution(r"C:\Users\davap\AppData"));
        assert!(caution(r"C:\Users\other\Downloads"));
    }

    #[test]
    fn ordinary_items_are_normal() {
        assert_eq!(c(r"C:\Users\davap\Downloads\a\setup.iso"), Protection::Normal);
        assert_eq!(c(r"C:\Users\davap\AppData\Local\Temp\x"), Protection::Normal);
        assert_eq!(c(r"C:\Windows.old"), Protection::Normal);
        assert_eq!(c(r"D:\SteamLibrary\steamapps\common\Game"), Protection::Normal);
        assert_eq!(c(r"C:\pagefile.sys.bak\x"), Protection::Normal);
        assert_eq!(c(r"D:\Boot\notes"), Protection::Normal);
    }

    #[test]
    fn links_are_never_deleted() {
        assert!(matches!(
            classify(r"C:\Users\davap\Downloads\junction", NodeKind::Link, &sys()),
            Protection::Blocked(_)
        ));
    }

    #[test]
    fn protection_holds_on_other_system_drives() {
        let s = SystemPaths {
            windows: Some(r"D:\Windows".to_owned()),
            program_files: vec![r"D:\Program Files".to_owned()],
            program_data: None,
            users: Some(r"D:\Users".to_owned()),
            profile: None,
        };
        assert!(matches!(classify(r"D:\Windows", NodeKind::Dir, &s), Protection::Blocked(_)));
        assert!(matches!(classify(r"D:\Windows\WinSxS", NodeKind::Dir, &s), Protection::Caution(_)));
        assert_eq!(classify(r"C:\Windows", NodeKind::Dir, &s), Protection::Normal);
    }
}
