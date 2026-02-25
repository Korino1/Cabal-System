use anyhow::{Result, bail};
use std::collections::BTreeSet;

pub fn default_allowed_ide_profiles() -> Vec<String> {
    vec![
        "generic".to_string(),
        "vscode".to_string(),
        "jetbrains".to_string(),
        "cursor".to_string(),
        "windsurf".to_string(),
        "zed".to_string(),
    ]
}

pub fn detect_ide_profile_from_client_name(client_name: Option<&str>) -> String {
    let Some(name) = client_name else {
        return "generic".to_string();
    };
    let lowered = name.trim().to_ascii_lowercase();
    if lowered.is_empty() {
        return "generic".to_string();
    }
    if lowered.contains("cursor") {
        return "cursor".to_string();
    }
    if lowered.contains("windsurf") {
        return "windsurf".to_string();
    }
    if lowered.contains("visual studio code") || lowered.contains("vscode") {
        return "vscode".to_string();
    }
    if lowered.contains("jetbrains")
        || lowered.contains("intellij")
        || lowered.contains("idea")
        || lowered.contains("rustrover")
        || lowered.contains("clion")
        || lowered.contains("goland")
        || lowered.contains("pycharm")
        || lowered.contains("webstorm")
        || lowered.contains("phpstorm")
    {
        return "jetbrains".to_string();
    }
    if lowered.contains("zed") {
        return "zed".to_string();
    }
    "generic".to_string()
}

pub fn normalize_allowed_ide_profiles(raw: &[String]) -> Result<Vec<String>> {
    if raw.is_empty() {
        bail!("allowed_profiles must not be empty");
    }

    let mut out = BTreeSet::new();
    for item in raw {
        let normalized = normalize_ide_profile(item)
            .ok_or_else(|| anyhow::anyhow!("unsupported ide profile: {item}"))?;
        out.insert(normalized);
    }
    Ok(out.into_iter().collect())
}

pub fn is_ide_profile_allowed(profile: &str, allowed: &[String]) -> bool {
    allowed.iter().any(|x| x == profile)
}

fn normalize_ide_profile(input: &str) -> Option<String> {
    let lowered = input.trim().to_ascii_lowercase();
    if lowered.is_empty() {
        return None;
    }
    let normalized = match lowered.as_str() {
        "generic" => "generic",
        "vscode" | "visual studio code" | "code" => "vscode",
        "jetbrains" | "intellij" | "idea" | "rustrover" | "clion" | "goland" | "pycharm"
        | "webstorm" | "phpstorm" => "jetbrains",
        "cursor" => "cursor",
        "windsurf" => "windsurf",
        "zed" => "zed",
        _ => return None,
    };
    Some(normalized.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_profile_prefers_known_ides() {
        assert_eq!(
            detect_ide_profile_from_client_name(Some("Visual Studio Code")),
            "vscode"
        );
        assert_eq!(
            detect_ide_profile_from_client_name(Some("JetBrains IntelliJ IDEA")),
            "jetbrains"
        );
        assert_eq!(
            detect_ide_profile_from_client_name(Some("Cursor Editor")),
            "cursor"
        );
        assert_eq!(
            detect_ide_profile_from_client_name(Some("Unknown IDE")),
            "generic"
        );
    }

    #[test]
    fn normalize_profiles_dedups_and_validates() {
        let out = normalize_allowed_ide_profiles(&[
            "Visual Studio Code".to_string(),
            "vscode".to_string(),
            "idea".to_string(),
        ])
        .expect("normalize");
        assert_eq!(out, vec!["jetbrains".to_string(), "vscode".to_string()]);
    }

    #[test]
    fn normalize_profiles_rejects_unknown() {
        let err = normalize_allowed_ide_profiles(&["mystery".to_string()]).expect_err("must fail");
        assert!(err.to_string().contains("unsupported ide profile"));
    }
}
