//! HTTP client of the RainbowMiner API (the engine's web server, Config "APIport")

use serde_json::Value;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct Endpoint {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
}

impl Endpoint {
    fn agent() -> ureq::Agent {
        // the engine runs on this machine or in the LAN: no proxy
        ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(3)).timeout(Duration::from_secs(10)).build()
    }

    pub fn url(&self, path: &str) -> String {
        format!("http://{}:{}{}", self.host, self.port, path)
    }

    fn request(&self, path: &str) -> ureq::Request {
        let mut r = Self::agent().get(&self.url(path));
        if !self.user.is_empty() {
            use base64::Engine;
            let token = base64::engine::general_purpose::STANDARD.encode(format!("{}:{}", self.user, self.password));
            r = r.set("Authorization", &format!("Basic {}", token));
        }
        r
    }

    /// GET a JSON endpoint
    pub fn json(&self, path: &str) -> Result<Value, String> {
        match self.request(path).call() {
            Ok(resp) => {
                let text = resp.into_string().map_err(|e| e.to_string())?;
                serde_json::from_str(&text).map_err(|e| format!("{}: {}", path, e))
            }
            Err(ureq::Error::Status(401, _)) => Err("401 Unauthorized (APIauth: check user and password)".into()),
            Err(ureq::Error::Status(code, _)) => Err(format!("{}: HTTP {}", path, code)),
            Err(e) => Err(e.to_string()),
        }
    }

    /// GET a command endpoint (the answer does not matter)
    pub fn command(&self, path: &str) -> Result<(), String> {
        self.request(path).call().map(|_| ()).map_err(|e| e.to_string())
    }
}
