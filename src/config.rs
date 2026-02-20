#[derive(Debug, Clone)]
pub struct Config {
    pub kibana_url: Option<String>,
    /// Optional: direct Elasticsearch endpoint for indexing chat history (e.g. https://localhost:9200).
    pub es_host: Option<String>,
    pub api_key: Option<String>,
    pub space: Option<String>,
    pub agent_id: String,
    /// Dev-only: allow self-signed certs / hostname mismatches (e.g. Kibana `--ssl` on localhost).
    pub insecure_tls: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            kibana_url: None,
            es_host: None,
            api_key: None,
            space: None,
            agent_id: "elastic-ai-agent".to_string(),
            insecure_tls: false,
        }
    }
}

impl Config {
    pub fn missing(&self) -> Vec<&'static str> {
        let mut missing = Vec::new();
        if self.kibana_url.as_deref().unwrap_or("").is_empty() {
            missing.push("KIBANA_URL (or ES_HOST)");
        }
        if self.api_key.as_deref().unwrap_or("").is_empty() {
            missing.push("API_KEY (or ES_API_KEY)");
        }
        missing
    }

    pub fn is_ready(&self) -> bool {
        self.missing().is_empty()
    }
}

pub fn load_from_env() -> Config {
    // Important on macOS/GUI launches (Cursor): the process may not inherit your shell env.
    // Loading a local `.env` makes dev behavior consistent.
    let _ = dotenvy::dotenv();

    let mut cfg = Config::default();

    cfg.kibana_url = env_first_nonempty(&["KIBANA_URL"]);
    cfg.es_host = env_first_nonempty(&["ES_HOST", "ELASTICSEARCH_HOST", "ELASTIC_HOST"]);
    cfg.api_key = env_first_nonempty(&["API_KEY"]);

    cfg.space = env_first_nonempty(&["KIBANA_SPACE", "SPACE"]);
    cfg.insecure_tls = env_bool(&["KIBANA_INSECURE_TLS", "INSECURE_TLS"], false);

    if let Ok(agent_id) = std::env::var("AGENT_ID") {
        let agent_id = agent_id.trim().to_string();
        if !agent_id.is_empty() {
            cfg.agent_id = agent_id;
        }
    }

    cfg
}

fn env_first_nonempty(keys: &[&str]) -> Option<String> {
    for k in keys {
        if let Ok(v) = std::env::var(k) {
            let v = v.trim();
            if !v.is_empty() {
                return Some(v.to_string());
            }
        }
    }
    None
}

fn env_bool(keys: &[&str], default: bool) -> bool {
    for k in keys {
        if let Ok(v) = std::env::var(k) {
            let v = v.trim().to_ascii_lowercase();
            if v.is_empty() {
                continue;
            }
            return matches!(v.as_str(), "1" | "true" | "yes" | "y" | "on");
        }
    }
    default
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    fn lock_env() -> std::sync::MutexGuard<'static, ()> {
        ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
    }

    fn set_env(key: &str, value: Option<&str>) {
        // Rust 2024 makes env mutation `unsafe` because it can be UB if other threads
        // read/write the environment concurrently. Our tests serialize env access via
        // `ENV_LOCK`, so this is safe within this test module's conventions.
        unsafe {
            match value {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
    }

    #[test]
    fn default_config_is_not_ready_and_reports_missing() {
        let cfg = Config::default();
        let missing = cfg.missing();
        assert!(missing.contains(&"KIBANA_URL (or ES_HOST)"));
        assert!(missing.contains(&"API_KEY (or ES_API_KEY)"));
        assert!(!cfg.is_ready());
    }

    #[test]
    fn is_ready_when_kibana_url_and_api_key_present() {
        let mut cfg = Config::default();
        cfg.kibana_url = Some("http://localhost:5601".to_string());
        cfg.api_key = Some("k".to_string());
        assert!(cfg.is_ready());
    }

    #[test]
    fn env_bool_parses_true_values_and_falls_back_to_default() {
        let _g = lock_env();

        set_env("TEST_BOOL", None);
        assert_eq!(env_bool(&["TEST_BOOL"], false), false);
        assert_eq!(env_bool(&["TEST_BOOL"], true), true);

        set_env("TEST_BOOL", Some("true"));
        assert!(env_bool(&["TEST_BOOL"], false));

        set_env("TEST_BOOL", Some("1"));
        assert!(env_bool(&["TEST_BOOL"], false));

        set_env("TEST_BOOL", Some("on"));
        assert!(env_bool(&["TEST_BOOL"], false));

        set_env("TEST_BOOL", Some("nope"));
        assert!(!env_bool(&["TEST_BOOL"], false));

        set_env("TEST_BOOL", None);
    }

    #[test]
    fn load_from_env_reads_expected_vars() {
        let _g = lock_env();

        // Save old values and restore at the end.
        let keys = [
            "KIBANA_URL",
            "ES_HOST",
            "API_KEY",
            "KIBANA_SPACE",
            "KIBANA_INSECURE_TLS",
            "AGENT_ID",
        ];
        let old: Vec<(String, Option<String>)> = keys
            .iter()
            .map(|k| ((*k).to_string(), std::env::var(k).ok()))
            .collect();

        set_env("KIBANA_URL", Some("http://127.0.0.1:5601"));
        set_env("ES_HOST", Some("http://127.0.0.1:9200"));
        set_env("API_KEY", Some("k"));
        set_env("KIBANA_SPACE", Some("default"));
        set_env("KIBANA_INSECURE_TLS", Some("true"));
        set_env("AGENT_ID", Some("my-agent"));

        let cfg = load_from_env();
        assert_eq!(cfg.kibana_url.as_deref(), Some("http://127.0.0.1:5601"));
        assert_eq!(cfg.es_host.as_deref(), Some("http://127.0.0.1:9200"));
        assert_eq!(cfg.api_key.as_deref(), Some("k"));
        assert_eq!(cfg.space.as_deref(), Some("default"));
        assert_eq!(cfg.insecure_tls, true);
        assert_eq!(cfg.agent_id, "my-agent");

        // Restore.
        for (k, v) in old {
            set_env(&k, v.as_deref());
        }
    }
}
