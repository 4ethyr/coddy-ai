use std::{collections::BTreeMap, fs, path::Path};

const CODDY_MANIFESTS: &[&str] = &[
    "apps/coddy/Cargo.toml",
    "crates/coddy-agent/Cargo.toml",
    "crates/coddy-client/Cargo.toml",
    "crates/coddy-core/Cargo.toml",
    "crates/coddy-ipc/Cargo.toml",
    "crates/coddy-runtime/Cargo.toml",
    "crates/coddy-voice-input/Cargo.toml",
];

const VISIONCLIP_RUNTIME_CRATES: &[&str] = &[
    "visionclip-common",
    "visionclip-infer",
    "visionclip-output",
    "visionclip-tts",
];

#[test]
fn coddy_manifests_do_not_add_unapproved_visionclip_dependencies() {
    let repo_root = repo_root();
    let temporary_allowlist: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let mut violations = Vec::new();

    for manifest in CODDY_MANIFESTS {
        let path = repo_root.join(manifest);
        if !path.exists() {
            continue;
        }

        let raw = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));

        for crate_name in VISIONCLIP_RUNTIME_CRATES {
            let allowed = temporary_allowlist
                .get(manifest)
                .is_some_and(|allowed| allowed.contains(crate_name));

            if !allowed && manifest_mentions_dependency(&raw, crate_name) {
                violations.push(format!("{manifest} depends on {crate_name}"));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Coddy packages must stay decoupled from VisionClip runtime crates:\n{}",
        violations.join("\n")
    );
}

#[test]
fn coddy_sources_do_not_include_visionclip_app_sources_or_runtime_crates() {
    let repo_root = repo_root();
    let mut violations = Vec::new();

    for source_dir in [
        "apps/coddy/src",
        "crates/coddy-agent/src",
        "crates/coddy-client/src",
        "crates/coddy-core/src",
        "crates/coddy-ipc/src",
        "crates/coddy-runtime/src",
        "crates/coddy-voice-input/src",
    ] {
        collect_source_violations(&repo_root.join(source_dir), &mut violations);
    }

    collect_coddy_electron_violations(repo_root, &mut violations);

    assert!(
        violations.is_empty(),
        "Coddy source must not couple to VisionClip app/runtime internals:\n{}",
        violations.join("\n")
    );
}

fn repo_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("coddy-ipc lives under crates/coddy-ipc")
}

fn manifest_mentions_dependency(raw: &str, crate_name: &str) -> bool {
    raw.lines().map(str::trim).any(|line| {
        if line.is_empty() || line.starts_with('#') {
            return false;
        }

        line.starts_with(&format!("{crate_name} "))
            || line.starts_with(&format!("{crate_name}="))
            || line.starts_with(&format!("{crate_name}."))
            || line == format!("[dependencies.{crate_name}]")
            || line == format!("[dev-dependencies.{crate_name}]")
            || line == format!("[build-dependencies.{crate_name}]")
            || line.contains(&format!("package = \"{crate_name}\""))
    })
}

fn collect_source_violations(dir: &Path, violations: &mut Vec<String>) {
    if !dir.exists() {
        return;
    }

    for entry in fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", dir.display()))
    {
        let entry = entry.unwrap_or_else(|error| panic!("failed to read dir entry: {error}"));
        let path = entry.path();

        if path.is_dir() {
            collect_source_violations(&path, violations);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            collect_file_violations(&path, violations);
        }
    }
}

fn collect_coddy_electron_violations(repo_root: &Path, violations: &mut Vec<String>) {
    for relative_path in [
        "apps/coddy-electron/package.json",
        "apps/coddy-electron/src/main/ipcBridge.ts",
    ] {
        let path = repo_root.join(relative_path);
        let raw = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));

        for forbidden in ["VISIONCLIP_CONFIG", "VISIONCLIP_RUNTIME_DIR"] {
            if raw.contains(forbidden) {
                violations.push(format!("{relative_path} contains `{forbidden}`"));
            }
        }
    }
}

fn collect_file_violations(path: &Path, violations: &mut Vec<String>) {
    let raw = fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));

    for pattern in [
        "include!(\"../../visionclip/",
        "include!(\"../../../apps/visionclip/",
        "visionclip_common::",
        "visionclip_infer::",
        "visionclip_output::",
        "visionclip_tts::",
    ] {
        if raw.contains(pattern) {
            violations.push(format!("{} contains `{pattern}`", path.display()));
        }
    }
}
