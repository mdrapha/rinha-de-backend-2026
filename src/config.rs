pub struct ApiConfig {
    pub sock_path: String,
}

pub struct ProxyConfig {
    pub upstreams: Vec<String>,
    pub port: u16,
    pub workers: usize,
}

pub fn api_config() -> ApiConfig {
    ApiConfig {
        sock_path: std::env::var("SOCK").unwrap_or_else(|_| "/run/sock/api.sock".to_string()),
    }
}

pub fn proxy_config() -> ProxyConfig {
    let upstreams = std::env::var("UPSTREAMS")
        .unwrap_or_else(|_| "/run/sock/api1.sock,/run/sock/api2.sock".to_string())
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();
    let port = std::env::var("PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(9999);
    let workers = std::env::var("WORKERS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    ProxyConfig { upstreams, port, workers }
}
