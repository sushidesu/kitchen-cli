use clap::Args;
use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const MAX_SCAN_DEPTH: usize = 4;
const MAX_DISPLAY_CANDIDATES: usize = 20;
const SKIP_DIR_NAMES: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "dist",
    "build",
    ".next",
    ".turbo",
    ".pnpm-store",
];

#[derive(Args)]
pub struct RepoArgs {
    /// Root paths to scan for repositories
    #[arg(value_name = "PATH")]
    paths: Vec<String>,
}

impl RepoArgs {
    pub fn run(self) {
        if let Err(err) = run_repo(self) {
            eprintln!("{err}");
            std::process::exit(1);
        }
    }
}

fn run_repo(args: RepoArgs) -> Result<(), String> {
    let cwd = env::current_dir().map_err(|e| format!("kitchen repo: failed to read cwd: {e}"))?;
    let home = env::var("HOME").ok().map(PathBuf::from);
    let config_roots = read_config_roots(home.as_deref());
    let roots = resolve_roots(&args.paths, &config_roots, &cwd, home.as_deref())?;

    let repositories = collect_repositories(&roots);
    if repositories.is_empty() {
        return Err("kitchen repo: no repositories found".to_string());
    }

    match select_incrementally(&repositories)? {
        Some(path) => {
            println!("{}", path.display());
            Ok(())
        }
        None => Ok(()),
    }
}

fn read_config_roots(home: Option<&Path>) -> Vec<String> {
    let Some(home) = home else {
        return Vec::new();
    };

    let config_path = home.join(".config/kitchen/config.toml");
    let Ok(content) = fs::read_to_string(&config_path) else {
        return Vec::new();
    };

    match parse_repo_roots_from_config(&content) {
        Ok(roots) => roots,
        Err(err) => {
            eprintln!(
                "kitchen repo: failed to parse {}: {err}",
                config_path.display()
            );
            Vec::new()
        }
    }
}

fn parse_repo_roots_from_config(content: &str) -> Result<Vec<String>, String> {
    let mut in_repo = false;

    for raw_line in content.lines() {
        let line_without_comment = raw_line.split('#').next().unwrap_or("");
        let line = line_without_comment.trim();
        if line.is_empty() {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            in_repo = &line[1..line.len() - 1] == "repo";
            continue;
        }

        if !in_repo {
            continue;
        }

        if let Some(value) = line.strip_prefix("roots") {
            let value = value.trim();
            let value = value
                .strip_prefix('=')
                .ok_or_else(|| "missing '=' after roots".to_string())?
                .trim();
            return parse_toml_string_array(value);
        }
    }

    Ok(Vec::new())
}

fn parse_toml_string_array(input: &str) -> Result<Vec<String>, String> {
    let array_body = input
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .ok_or_else(|| "roots must be a single-line TOML string array".to_string())?;

    let trimmed = array_body.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    let mut items = Vec::new();
    for part in trimmed.split(',') {
        let value = part.trim();
        let value = value
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .ok_or_else(|| "roots entries must be quoted strings".to_string())?;
        items.push(value.to_string());
    }

    Ok(items)
}

fn resolve_roots(
    cli_paths: &[String],
    config_roots: &[String],
    cwd: &Path,
    home: Option<&Path>,
) -> Result<Vec<PathBuf>, String> {
    let raw_roots = if !cli_paths.is_empty() {
        cli_paths
    } else {
        config_roots
    };

    let mut roots = Vec::new();
    for raw in raw_roots {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        roots.push(normalize_path(trimmed, cwd, home)?);
    }

    if roots.is_empty() {
        roots.push(cwd.to_path_buf());
    }

    Ok(roots)
}

fn normalize_path(input: &str, cwd: &Path, home: Option<&Path>) -> Result<PathBuf, String> {
    let expanded = expand_home(input, home)?;
    let path = PathBuf::from(expanded);
    if path.is_absolute() {
        Ok(path)
    } else {
        Ok(cwd.join(path))
    }
}

fn expand_home(input: &str, home: Option<&Path>) -> Result<String, String> {
    if input == "~" {
        let home = home.ok_or_else(|| "kitchen repo: HOME is not set".to_string())?;
        return Ok(home.display().to_string());
    }

    if let Some(rest) = input.strip_prefix("~/") {
        let home = home.ok_or_else(|| "kitchen repo: HOME is not set".to_string())?;
        return Ok(home.join(rest).display().to_string());
    }

    Ok(input.to_string())
}

fn collect_repositories(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut repositories = BTreeSet::new();

    for root in roots {
        scan_for_repositories(root, 0, &mut repositories);
    }

    repositories.into_iter().collect()
}

fn scan_for_repositories(dir: &Path, depth: usize, repositories: &mut BTreeSet<PathBuf>) {
    if depth > MAX_SCAN_DEPTH {
        return;
    }

    if is_git_repository(dir) {
        repositories.insert(dir.to_path_buf());
        return;
    }

    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(_) => continue,
        };

        if !file_type.is_dir() {
            continue;
        }

        if should_skip_dir(&path) {
            continue;
        }

        scan_for_repositories(&path, depth + 1, repositories);
    }
}

fn is_git_repository(path: &Path) -> bool {
    let git_path = path.join(".git");
    git_path.is_dir() || git_path.is_file()
}

fn should_skip_dir(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };

    SKIP_DIR_NAMES.contains(&name)
}

fn select_incrementally(repositories: &[PathBuf]) -> Result<Option<PathBuf>, String> {
    let mut query = String::new();

    loop {
        let filtered = filter_repositories(repositories, &query);

        eprintln!();
        if query.is_empty() {
            eprintln!("kitchen repo: repositories");
        } else {
            eprintln!("kitchen repo: repositories (query: {query})");
        }

        if filtered.is_empty() {
            eprintln!("  no matches");
        } else {
            for (index, path) in filtered.iter().take(MAX_DISPLAY_CANDIDATES).enumerate() {
                eprintln!("{:>2}. {}", index + 1, path.display());
            }
            if filtered.len() > MAX_DISPLAY_CANDIDATES {
                eprintln!("  ... and {} more", filtered.len() - MAX_DISPLAY_CANDIDATES);
            }
        }

        eprintln!("input query to refine, number to select, or empty line to cancel:");

        let mut input = String::new();
        std::io::stdin()
            .read_line(&mut input)
            .map_err(|e| format!("kitchen repo: failed to read input: {e}"))?;
        let input = input.trim();

        if input.is_empty() {
            return Ok(None);
        }

        if let Ok(index) = input.parse::<usize>() {
            if index == 0 || index > filtered.len() {
                eprintln!("kitchen repo: invalid selection index: {index}");
                continue;
            }
            return Ok(Some(filtered[index - 1].clone()));
        }

        query = input.to_string();
    }
}

fn filter_repositories(repositories: &[PathBuf], query: &str) -> Vec<PathBuf> {
    let normalized_query = query.trim().to_lowercase();
    if normalized_query.is_empty() {
        return repositories.to_vec();
    }

    let terms: Vec<&str> = normalized_query.split_whitespace().collect();
    repositories
        .iter()
        .filter(|path| {
            let haystack = path.to_string_lossy().to_lowercase();
            terms.iter().all(|term| haystack.contains(term))
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        expand_home, filter_repositories, is_git_repository, parse_repo_roots_from_config,
        resolve_roots,
    };
    use std::fs;
    use std::path::{Path, PathBuf};

    #[test]
    fn resolve_roots_prefers_cli_over_config() {
        let cwd = Path::new("/tmp/work");
        let home = Path::new("/Users/test");
        let roots = resolve_roots(
            &["~/dev".to_string()],
            &["~/other".to_string()],
            cwd,
            Some(home),
        )
        .expect("roots should resolve");

        assert_eq!(roots, vec![PathBuf::from("/Users/test/dev")]);
    }

    #[test]
    fn resolve_roots_uses_config_when_cli_is_empty() {
        let cwd = Path::new("/tmp/work");
        let home = Path::new("/Users/test");
        let roots = resolve_roots(&[], &["./repos".to_string()], cwd, Some(home))
            .expect("roots should resolve");

        assert_eq!(roots, vec![PathBuf::from("/tmp/work/./repos")]);
    }

    #[test]
    fn resolve_roots_falls_back_to_cwd_when_all_empty() {
        let cwd = Path::new("/tmp/work");
        let roots = resolve_roots(&[], &[], cwd, None).expect("roots should resolve");
        assert_eq!(roots, vec![PathBuf::from("/tmp/work")]);
    }

    #[test]
    fn expand_home_expands_tilde_prefix() {
        let expanded = expand_home("~/dev", Some(Path::new("/Users/test"))).expect("expanded");
        assert_eq!(expanded, "/Users/test/dev");
    }

    #[test]
    fn parse_repo_roots_from_config_reads_repo_roots() {
        let config = r#"
            [repo]
            roots = ["/a/dev", "/b/work"]
        "#;
        let roots = parse_repo_roots_from_config(config).expect("config should parse");
        assert_eq!(roots, vec!["/a/dev".to_string(), "/b/work".to_string()]);
    }

    #[test]
    fn parse_repo_roots_from_config_returns_empty_when_missing() {
        let config = r#"
            [notify]
            title = "kitchen"
        "#;
        let roots = parse_repo_roots_from_config(config).expect("config should parse");
        assert!(roots.is_empty());
    }

    #[test]
    fn filter_repositories_matches_all_query_terms() {
        let repositories = vec![
            PathBuf::from("/Users/test/dev/kitchen-cli"),
            PathBuf::from("/Users/test/dev/dotfiles"),
        ];

        let filtered = filter_repositories(&repositories, "dev kitchen");
        assert_eq!(filtered, vec![PathBuf::from("/Users/test/dev/kitchen-cli")]);
    }

    #[test]
    fn is_git_repository_true_for_dot_git_directory() {
        let temp_root = std::env::temp_dir().join(format!("kitchen-test-{}", std::process::id()));
        let repo_dir = temp_root.join("repo");
        let git_dir = repo_dir.join(".git");

        fs::create_dir_all(&git_dir).expect("create test repo");
        assert!(is_git_repository(&repo_dir));

        fs::remove_dir_all(&temp_root).expect("cleanup test repo");
    }
}
