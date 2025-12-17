use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;
use tracing::{debug, info};

/// Detected build system for an extension
#[derive(Debug, Clone)]
pub enum BuildSystem {
    /// npm with a build script
    Npm { script: String },
    /// pnpm with a build script
    Pnpm { script: String },
    /// yarn with a build script
    Yarn { script: String },
    /// Make
    Make,
    /// web-ext
    WebExt,
    /// No build needed (plain extension)
    None,
    /// Custom command
    Custom { command: String },
}

impl BuildSystem {
    /// Detect the build system from a directory
    pub fn detect(dir: &Path) -> Result<Self> {
        // Check for package.json
        let package_json = dir.join("package.json");
        if package_json.exists() {
            let content = std::fs::read_to_string(&package_json)?;
            let pkg: serde_json::Value = serde_json::from_str(&content)?;

            // Check for build script
            if let Some(scripts) = pkg.get("scripts").and_then(|s| s.as_object()) {
                let build_script = if scripts.contains_key("build") {
                    Some("build")
                } else if scripts.contains_key("dist") {
                    Some("dist")
                } else if scripts.contains_key("package") {
                    Some("package")
                } else {
                    None
                };

                if let Some(script) = build_script {
                    // Detect package manager
                    if dir.join("pnpm-lock.yaml").exists() {
                        return Ok(BuildSystem::Pnpm {
                            script: script.to_string(),
                        });
                    } else if dir.join("yarn.lock").exists() {
                        return Ok(BuildSystem::Yarn {
                            script: script.to_string(),
                        });
                    } else {
                        return Ok(BuildSystem::Npm {
                            script: script.to_string(),
                        });
                    }
                }
            }
        }

        // Check for Makefile
        if dir.join("Makefile").exists() || dir.join("makefile").exists() {
            return Ok(BuildSystem::Make);
        }

        // Check for web-ext config
        if dir.join("web-ext-config.js").exists() || dir.join("web-ext-config.cjs").exists() {
            return Ok(BuildSystem::WebExt);
        }

        // No build system detected - might be a plain extension
        if dir.join("manifest.json").exists() {
            return Ok(BuildSystem::None);
        }

        anyhow::bail!("Could not detect build system and no manifest.json found")
    }

    /// Get the build command as a string (for storage/display)
    pub fn to_command_string(&self) -> Option<String> {
        match self {
            BuildSystem::Npm { script } => Some(format!("npm install && npm run {}", script)),
            BuildSystem::Pnpm { script } => Some(format!("pnpm install && pnpm run {}", script)),
            BuildSystem::Yarn { script } => Some(format!("yarn install && yarn {}", script)),
            BuildSystem::Make => Some("make".to_string()),
            BuildSystem::WebExt => Some("web-ext build".to_string()),
            BuildSystem::None => None,
            BuildSystem::Custom { command } => Some(command.clone()),
        }
    }
}

/// Clone a git repository
pub fn clone_repo(url: &str, ref_spec: &str, target_dir: &Path) -> Result<()> {
    info!("Cloning {} (ref: {})", url, ref_spec);

    // Clone the repo
    let status = Command::new("git")
        .args(["clone", "--depth", "1", "--branch", ref_spec, url])
        .arg(target_dir)
        .status()
        .context("Failed to run git clone")?;

    if !status.success() {
        // Try without --branch (might be a commit hash)
        let status = Command::new("git")
            .args(["clone", url])
            .arg(target_dir)
            .status()
            .context("Failed to run git clone")?;

        if !status.success() {
            anyhow::bail!("git clone failed");
        }

        // Checkout the specific ref
        let status = Command::new("git")
            .args(["checkout", ref_spec])
            .current_dir(target_dir)
            .status()
            .context("Failed to checkout ref")?;

        if !status.success() {
            anyhow::bail!("git checkout {} failed", ref_spec);
        }
    }

    Ok(())
}

/// Run the build for an extension
#[allow(clippy::cognitive_complexity)] // Match arms for each build system
pub fn run_build(dir: &Path, build_system: &BuildSystem) -> Result<()> {
    match build_system {
        BuildSystem::Npm { script } => run_js_build(dir, "npm", script),
        BuildSystem::Pnpm { script } => run_js_build(dir, "pnpm", script),
        BuildSystem::Yarn { script } => run_js_build(dir, "yarn", script),
        BuildSystem::Make => run_command(dir, "make", &[], "make"),
        BuildSystem::WebExt => run_command(dir, "web-ext", &["build"], "web-ext build"),
        BuildSystem::Custom { command } => {
            info!("Running custom build: {}", command);
            run_command(dir, "sh", &["-c", command], "custom build command")
        }
        BuildSystem::None => {
            debug!("No build step needed");
            Ok(())
        }
    }
}

fn run_js_build(dir: &Path, pm: &str, script: &str) -> Result<()> {
    run_command(dir, pm, &["install"], &format!("{} install", pm))?;
    // yarn uses `yarn <script>`, npm/pnpm use `<pm> run <script>`
    if pm == "yarn" {
        run_command(dir, pm, &[script], &format!("yarn {}", script))
    } else {
        run_command(dir, pm, &["run", script], &format!("{} run {}", pm, script))
    }
}

fn run_command(dir: &Path, cmd: &str, args: &[&str], description: &str) -> Result<()> {
    info!("Running {}", description);
    let status = Command::new(cmd)
        .args(args)
        .current_dir(dir)
        .status()
        .with_context(|| format!("Failed to run {}", description))?;

    if !status.success() {
        anyhow::bail!("{} failed", description);
    }
    Ok(())
}

/// Find the manifest.json in a built extension directory
pub fn find_manifest(dir: &Path) -> Result<PathBuf> {
    // Common output directories
    let candidates = [
        "dist",
        "build",
        "extension",
        "addon",
        "web-ext-artifacts",
        "pkg",
        "out",
        ".", // root
    ];

    for candidate in candidates {
        let manifest = dir.join(candidate).join("manifest.json");
        if manifest.exists() {
            return Ok(dir.join(candidate));
        }
    }

    // Search recursively (limited depth)
    for entry in walkdir(dir, 3)? {
        if entry
            .file_name()
            .map(|n| n == "manifest.json")
            .unwrap_or(false)
            && let Some(parent) = entry.parent()
        {
            return Ok(parent.to_path_buf());
        }
    }

    anyhow::bail!("Could not find manifest.json in build output")
}

/// Simple directory walker with depth limit
fn walkdir(dir: &Path, max_depth: usize) -> Result<Vec<PathBuf>> {
    let mut results = Vec::new();
    walkdir_impl(dir, 0, max_depth, &mut results)?;
    Ok(results)
}

fn walkdir_impl(
    dir: &Path,
    depth: usize,
    max_depth: usize,
    results: &mut Vec<PathBuf>,
) -> Result<()> {
    if depth > max_depth {
        return Ok(());
    }

    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() {
            results.push(path);
        } else if path.is_dir() {
            // Skip common non-extension directories
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if ![
                "node_modules",
                ".git",
                ".github",
                "test",
                "tests",
                "__pycache__",
            ]
            .contains(&name)
            {
                walkdir_impl(&path, depth + 1, max_depth, results)?;
            }
        }
    }

    Ok(())
}
