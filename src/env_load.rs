/// Standalone-first lookup. Optional backend paths remain as migration fallbacks.
const ENV_CANDIDATES: &[&str] = &[".env", "../backend/.env", "backend/.env"];

pub fn resolve_env_path_from<F>(exists: F) -> Option<&'static str>
where
    F: Fn(&str) -> bool,
{
    ENV_CANDIDATES.iter().copied().find(|path| exists(path))
}

pub fn resolve_env_path() -> Option<&'static str> {
    resolve_env_path_from(|path| std::path::Path::new(path).exists())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_root_env_over_backend_paths() {
        let exists = |p: &str| {
            p == ".env" || p == "../backend/.env" || p == "backend/.env"
        };
        assert_eq!(resolve_env_path_from(exists), Some(".env"));
    }

    #[test]
    fn falls_back_to_parent_backend_env() {
        let exists = |p: &str| p == "../backend/.env";
        assert_eq!(resolve_env_path_from(exists), Some("../backend/.env"));
    }

    #[test]
    fn falls_back_to_nested_backend_env() {
        let exists = |p: &str| p == "backend/.env";
        assert_eq!(resolve_env_path_from(exists), Some("backend/.env"));
    }

    #[test]
    fn returns_none_when_no_env_file_exists() {
        let exists = |_p: &str| false;
        assert_eq!(resolve_env_path_from(exists), None);
    }
}
