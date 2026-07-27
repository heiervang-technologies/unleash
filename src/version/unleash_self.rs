use super::version_compare;
use super::VersionManager;

impl VersionManager {
    pub fn get_unleash_version_list(&self) -> Vec<crate::version::VersionInfo> {
        let installed = env!("CARGO_PKG_VERSION");
        let mut versions: Vec<String> = Vec::new();

        if let Ok(output) = std::process::Command::new("gh")
            .args([
                "api",
                "repos/heiervang-technologies/unleash/releases",
                "--jq",
                ".[].tag_name",
            ])
            .output()
        {
            if output.status.success() {
                let tag_output = String::from_utf8_lossy(&output.stdout);
                versions = tag_output
                    .lines()
                    .map(|l| l.trim_start_matches('v').to_string())
                    .filter(|v| !v.is_empty() && v.starts_with(|c: char| c.is_ascii_digit()))
                    .collect();
            }
        }

        if versions.is_empty() {
            // Fall back to just showing the installed version
            return vec![crate::version::VersionInfo {
                version: installed.to_string(),
                is_installed: true,
            }];
        }

        versions.sort_by(|a, b| version_compare(b, a));
        versions.truncate(20);

        versions
            .into_iter()
            .map(|v| crate::version::VersionInfo {
                is_installed: v == installed,
                version: v,
            })
            .collect()
    }
}
