use clap::Args;
use crossterm::cursor;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::queue;
use crossterm::style::{Attribute, Color, SetAttribute, SetForegroundColor};
use crossterm::terminal::{self, ClearType};
use std::cmp::Reverse;
use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};

const MAX_SCAN_DEPTH: usize = 4;
const MIN_VISIBLE_CANDIDATES: usize = 6;
const MAX_VISIBLE_CANDIDATES: usize = 20;
const MATCH_FG: Color = Color::DarkYellow;
const MARKER_FG: Color = MATCH_FG;
const SELECTED_FG: Color = Color::White;
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
    if !input.starts_with('~') {
        return Ok(input.to_string());
    }
    let home = home.ok_or_else(|| "kitchen repo: HOME is not set".to_string())?;
    if let Some(rest) = input.strip_prefix("~/") {
        Ok(home.join(rest).display().to_string())
    } else {
        Ok(home.display().to_string())
    }
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

#[derive(Debug, Clone)]
struct Candidate {
    path: PathBuf,
    text: String,
    lower: String,
}

impl Candidate {
    fn new(path: PathBuf) -> Self {
        let text = path.display().to_string();
        let lower = text.to_lowercase();
        Self { path, text, lower }
    }
}

#[derive(Debug, Clone)]
struct MatchScore {
    index: usize,
    score: i64,
    positions: Vec<usize>,
}

#[derive(Debug)]
struct SelectorState {
    query: String,
    query_terms: Vec<String>,
    selected: usize,
    scroll: usize,
    visible_rows: usize,
    matches: Vec<MatchScore>,
}

impl SelectorState {
    fn new() -> Self {
        Self {
            query: String::new(),
            query_terms: Vec::new(),
            selected: 0,
            scroll: 0,
            visible_rows: MAX_VISIBLE_CANDIDATES,
            matches: Vec::new(),
        }
    }

    fn refresh_matches(&mut self, candidates: &[Candidate]) {
        self.query_terms = self.query.split_whitespace().map(|t| t.to_lowercase()).collect();
        self.matches = fuzzy_match_candidates(candidates, &self.query_terms);
        if self.selected >= self.matches.len() {
            self.selected = self.matches.len().saturating_sub(1);
        }
        self.adjust_scroll();
    }

    fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
            self.adjust_scroll();
        }
    }

    fn move_down(&mut self) {
        if self.selected + 1 < self.matches.len() {
            self.selected += 1;
            self.adjust_scroll();
        }
    }

    fn adjust_scroll(&mut self) {
        if self.selected < self.scroll {
            self.scroll = self.selected;
        }

        let window_end = self.scroll + self.visible_rows;
        if self.selected >= window_end {
            self.scroll = self.selected + 1 - self.visible_rows;
        }
    }
}

struct RawModeGuard;

impl RawModeGuard {
    fn new() -> Result<Self, String> {
        terminal::enable_raw_mode()
            .map_err(|e| format!("kitchen repo: failed to enable raw mode: {e}"))?;
        Ok(Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
    }
}

fn select_incrementally(repositories: &[PathBuf]) -> Result<Option<PathBuf>, String> {
    if !io::stdin().is_terminal() || !io::stderr().is_terminal() {
        return Err("kitchen repo: interactive mode requires a TTY".to_string());
    }

    let candidates: Vec<Candidate> = repositories
        .iter()
        .map(|p| Candidate::new(p.clone()))
        .collect();

    let _raw_mode = RawModeGuard::new()?;
    let mut stderr = io::stderr();
    let mut state = SelectorState::new();
    state.visible_rows = visible_rows();
    state.refresh_matches(&candidates);

    loop {
        draw_selector(&mut stderr, &state, &candidates)?;

        let event =
            event::read().map_err(|e| format!("kitchen repo: failed to read key event: {e}"))?;

        match event {
            Event::Resize(_, _) => {
                state.visible_rows = visible_rows();
                state.adjust_scroll();
            }
            Event::Key(key) => {
                if !should_handle_key(key) {
                    continue;
                }

                if is_cancel_key(key) {
                    clear_selector(&mut stderr)?;
                    return Ok(None);
                }

                match key {
                    KeyEvent {
                        code: KeyCode::Enter,
                        ..
                    } => {
                        clear_selector(&mut stderr)?;
                        let Some(selected) = state.matches.get(state.selected) else {
                            return Ok(None);
                        };
                        return Ok(Some(candidates[selected.index].path.clone()));
                    }
                    KeyEvent {
                        code: KeyCode::Up, ..
                    } => {
                        state.move_up();
                    }
                    KeyEvent {
                        code: KeyCode::Down,
                        ..
                    } => {
                        state.move_down();
                    }
                    KeyEvent {
                        code: KeyCode::Backspace,
                        ..
                    } => {
                        state.query.pop();
                        state.refresh_matches(&candidates);
                    }
                    KeyEvent {
                        code: KeyCode::Char(ch),
                        modifiers,
                        ..
                    } if modifiers == KeyModifiers::NONE || modifiers == KeyModifiers::SHIFT => {
                        state.query.push(ch);
                        state.refresh_matches(&candidates);
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

fn should_handle_key(key: KeyEvent) -> bool {
    match key.kind {
        KeyEventKind::Press | KeyEventKind::Repeat => true,
        KeyEventKind::Release => false,
    }
}

fn is_cancel_key(key: KeyEvent) -> bool {
    key.code == KeyCode::Esc
        || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL))
}

fn visible_rows() -> usize {
    let rows = terminal::size().map(|(_, h)| h as usize).unwrap_or(24);
    let reserved = 4usize;
    rows.saturating_sub(reserved)
        .clamp(MIN_VISIBLE_CANDIDATES, MAX_VISIBLE_CANDIDATES)
}

fn render_err(e: impl std::fmt::Display) -> String {
    format!("kitchen repo: failed to render selector: {e}")
}

fn reset_screen(stderr: &mut io::Stderr) -> Result<(), String> {
    queue!(stderr, cursor::MoveTo(0, 0), terminal::Clear(ClearType::All)).map_err(render_err)
}

fn draw_selector(
    stderr: &mut io::Stderr,
    state: &SelectorState,
    candidates: &[Candidate],
) -> Result<(), String> {
    reset_screen(stderr)?;

    write_line(stderr, &format!("repo> {}", state.query))?;
    write_line(
        stderr,
        &format!("matches: {} / {}", state.matches.len(), candidates.len()),
    )?;

    if state.matches.is_empty() {
        write_line(stderr, "  (no matches)")?;
    } else {
        let end = (state.scroll + state.visible_rows).min(state.matches.len());
        for row in state.scroll..end {
            let matched = &state.matches[row];
            write_candidate_line(
                stderr,
                &candidates[matched.index],
                &matched.positions,
                row == state.selected,
            )?;
        }
    }

    write_line(
        stderr,
        "[Enter] select  [Esc/Ctrl-C] cancel  [Up/Down] move",
    )?;

    stderr
        .flush()
        .map_err(|e| format!("kitchen repo: failed to flush selector: {e}"))
}

fn write_line(stderr: &mut io::Stderr, line: &str) -> Result<(), String> {
    write!(stderr, "{line}\r\n")
        .map_err(|e| format!("kitchen repo: failed to render selector: {e}"))
}

fn write_candidate_line(
    stderr: &mut io::Stderr,
    candidate: &Candidate,
    match_positions: &[usize],
    selected: bool,
) -> Result<(), String> {
    let text = &candidate.text;
    let marker = if selected { '>' } else { ' ' };

    queue!(
        stderr,
        SetAttribute(Attribute::NormalIntensity),
        SetForegroundColor(Color::Reset)
    )
    .map_err(render_err)?;

    if selected {
        queue!(
            stderr,
            SetAttribute(Attribute::Bold),
            SetForegroundColor(MARKER_FG)
        )
        .map_err(render_err)?;
    }
    write!(stderr, "{marker}").map_err(render_err)?;
    if selected {
        queue!(stderr, SetForegroundColor(SELECTED_FG)).map_err(render_err)?;
    } else {
        queue!(stderr, SetForegroundColor(Color::Reset)).map_err(render_err)?;
    }
    write!(stderr, " ").map_err(render_err)?;

    for (idx, ch) in text.chars().enumerate() {
        if match_positions.binary_search(&idx).is_ok() {
            queue!(stderr, SetForegroundColor(MATCH_FG)).map_err(render_err)?;
        } else if selected {
            queue!(stderr, SetForegroundColor(SELECTED_FG)).map_err(render_err)?;
        } else {
            queue!(stderr, SetForegroundColor(Color::Reset)).map_err(render_err)?;
        }
        write!(stderr, "{ch}").map_err(render_err)?;
    }

    queue!(
        stderr,
        SetAttribute(Attribute::NormalIntensity),
        SetForegroundColor(Color::Reset)
    )
    .map_err(render_err)?;
    write!(stderr, "\r\n").map_err(render_err)
}

fn clear_selector(stderr: &mut io::Stderr) -> Result<(), String> {
    reset_screen(stderr)?;
    stderr
        .flush()
        .map_err(|e| format!("kitchen repo: failed to flush selector: {e}"))
}

fn fuzzy_match_candidates(candidates: &[Candidate], terms: &[String]) -> Vec<MatchScore> {
    let mut scored = Vec::new();

    for (index, candidate) in candidates.iter().enumerate() {
        let maybe_total = terms
            .iter()
            .try_fold(0i64, |acc, term| fuzzy_score(&candidate.lower, term).map(|s| acc + s));
        if let Some(total) = maybe_total {
            let positions = match_positions_for_terms(&candidate.lower, terms);
            scored.push(MatchScore {
                index,
                score: total - candidate.lower.len() as i64,
                positions,
            });
        }
    }

    scored.sort_by_key(|m| {
        (
            Reverse(m.score),
            candidates[m.index].lower.len(),
            &candidates[m.index].lower,
        )
    });

    scored
}

fn match_positions_for_terms(haystack: &str, terms: &[String]) -> Vec<usize> {
    let mut positions = Vec::new();
    for term in terms {
        let Some(matched) = fuzzy_match_positions(haystack, term) else {
            return Vec::new();
        };
        positions.extend(matched);
    }
    positions.sort_unstable();
    positions.dedup();
    positions
}

fn find_subsequence(h: &[char], q: &[char]) -> Option<Vec<usize>> {
    let mut cursor = 0;
    let mut positions = Vec::with_capacity(q.len());
    for &needle in q {
        let pos = h[cursor..].iter().position(|&ch| ch == needle)? + cursor;
        positions.push(pos);
        cursor = pos + 1;
    }
    Some(positions)
}

fn fuzzy_match_positions(haystack: &str, query: &str) -> Option<Vec<usize>> {
    if query.is_empty() {
        return Some(Vec::new());
    }
    let h: Vec<char> = haystack.chars().collect();
    let q: Vec<char> = query.chars().collect();
    find_subsequence(&h, &q)
}

fn fuzzy_score(haystack: &str, query: &str) -> Option<i64> {
    if query.is_empty() {
        return Some(0);
    }
    let h: Vec<char> = haystack.chars().collect();
    let q: Vec<char> = query.chars().collect();
    let positions = find_subsequence(&h, &q)?;

    let mut score = (positions.len() as i64) * 10;
    for (i, &pos) in positions.iter().enumerate() {
        if i > 0 && pos == positions[i - 1] + 1 {
            score += 15;
        }
        if pos == 0 || is_word_boundary(h[pos - 1]) {
            score += 8;
        }
    }
    if let Some(&first) = positions.first() {
        score += 25 - (first as i64).min(25);
    }
    Some(score)
}

fn is_word_boundary(ch: char) -> bool {
    matches!(ch, '/' | '-' | '_' | '.' | ' ')
}

#[cfg(test)]
mod tests {
    use super::{
        expand_home, fuzzy_match_candidates, fuzzy_match_positions, fuzzy_score, is_git_repository,
        match_positions_for_terms, parse_repo_roots_from_config, resolve_roots,
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
    fn fuzzy_score_matches_subsequence() {
        assert!(fuzzy_score("/users/me/dev/kitchen-cli", "ktcn").is_some());
        assert!(fuzzy_score("/users/me/dev/kitchen-cli", "zzz").is_none());
    }

    #[test]
    fn fuzzy_match_positions_returns_character_indices() {
        let positions = fuzzy_match_positions("kitchen-cli", "kcn").expect("should match");
        assert_eq!(positions, vec![0, 3, 6]);
    }

    #[test]
    fn match_positions_for_terms_merges_terms() {
        let terms = vec!["kit".to_string(), "cli".to_string()];
        let positions = match_positions_for_terms("/users/me/dev/kitchen-cli", &terms);
        assert!(!positions.is_empty());
        assert!(positions.contains(&14));
    }

    #[test]
    fn fuzzy_match_candidates_ranks_better_match_first() {
        let candidates = vec![
            super::Candidate {
                path: PathBuf::from("/users/me/dev/kitchen-cli"),
                text: "/users/me/dev/kitchen-cli".to_string(),
                lower: "/users/me/dev/kitchen-cli".to_string(),
            },
            super::Candidate {
                path: PathBuf::from("/users/me/dev/kitten"),
                text: "/users/me/dev/kitten".to_string(),
                lower: "/users/me/dev/kitten".to_string(),
            },
        ];

        let terms = vec!["kitchen".to_string()];
        let scored = fuzzy_match_candidates(&candidates, &terms);
        assert_eq!(scored[0].index, 0);
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
