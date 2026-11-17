//! OVHcloud API client: application key/secret + consumer key, `$1$` SHA1 signature.
use serde_json::Value;
use sha1::{Digest, Sha1};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const ENDPOINT: &str = "https://eu.api.ovh.com/1.0";

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Creds {
    pub app_key: String,
    pub app_secret: String,
    pub consumer_key: String,
}

/// `$1$` + sha1_hex(AS+CK+METHOD+URL+BODY+TIMESTAMP joined by '+').
pub fn sign(secret: &str, ck: &str, method: &str, url: &str, body: &str, ts: i64) -> String {
    let mut h = Sha1::new();
    h.update(format!("{secret}+{ck}+{method}+{url}+{body}+{ts}"));
    format!("$1${:x}", h.finalize())
}

/// Un segmento di percorso o un valore di query: tutto tranne `A-Za-z0-9-_.~` diventa `%XX`
/// (un blocco IP come `1.2.3.4/32` deve viaggiare come `1.2.3.4%2F32`).
pub fn enc(s: &str) -> String {
    s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

#[derive(Clone)]
pub struct Client {
    agent: ureq::Agent,
    creds: Creds,
    /// server clock minus local clock, from /auth/time (a wrong PC clock breaks the signature)
    delta: i64,
}

impl Client {
    pub fn connect(creds: Creds) -> Result<Self, String> {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(20)))
            .build()
            .into();
        let mut c = Client { agent, creds, delta: 0 };
        let t = c.raw("GET", "/auth/time", None, false)?;
        c.delta = t.as_i64().ok_or("risposta /auth/time non valida")? - now();
        Ok(c)
    }

    pub fn get(&self, path: &str) -> Result<Value, String> {
        self.raw("GET", path, None, true)
    }
    pub fn post(&self, path: &str, body: Option<Value>) -> Result<Value, String> {
        self.raw("POST", path, body, true)
    }
    /// Chiamata generica per le azioni confermate dalla UI.
    pub fn call(&self, method: &str, path: &str, body: Option<Value>) -> Result<Value, String> {
        self.raw(method, path, body, true)
    }
    /// ora del server OVH, in secondi
    pub fn server_now(&self) -> i64 {
        now() + self.delta
    }

    /// Serie temporale da Mimir (Prometheus), endpoint e token da /vps/{s}/metricsToken.
    /// La query deve contenere `service_name="..."` (LBAC) o Mimir risponde 400.
    pub fn prom_range(&self, ep: &str, token: &str, query: &str, start: i64, end: i64, step: i64) -> Result<Vec<(i64, f64)>, String> {
        if !ep.starts_with("https://") {
            return Err(format!("endpoint metriche inatteso: {ep}"));
        }
        let q = enc(query);
        let url = format!("{}/prometheus/api/v1/query_range?query={q}&start={start}&end={end}&step={step}", ep.trim_end_matches('/'));
        let mut resp = self.agent.get(&url).header("Authorization", format!("Bearer {token}")).call().map_err(|e| format!("rete: {e}"))?;
        let status = resp.status().as_u16();
        let text = resp.body_mut().read_to_string().map_err(|e| format!("lettura risposta: {e}"))?;
        let j: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        if status >= 400 {
            return Err(format!("{status}: {}", j["error"].as_str().unwrap_or(&text)));
        }
        let vals = j["data"]["result"][0]["values"].as_array();
        Ok(vals.map(|a| a.iter().filter_map(|p| Some((p[0].as_f64()? as i64, p[1].as_str()?.parse().ok()?))).collect()).unwrap_or_default())
    }

    fn raw(&self, method: &str, path: &str, body: Option<Value>, signed: bool) -> Result<Value, String> {
        let url = format!("{ENDPOINT}{path}");
        let body = body.map(|b| b.to_string()).unwrap_or_default();
        let ts = now() + self.delta;
        let sig = sign(&self.creds.app_secret, &self.creds.consumer_key, method, &url, &body, ts);
        let creds = &self.creds;
        macro_rules! hdr {
            ($r:expr) => {{
                let r = $r.header("X-Ovh-Application", &creds.app_key).header("Content-Type", "application/json");
                if signed {
                    r.header("X-Ovh-Consumer", &creds.consumer_key)
                        .header("X-Ovh-Timestamp", ts.to_string())
                        .header("X-Ovh-Signature", &sig)
                } else {
                    r
                }
            }};
        }
        let res = match method {
            "GET" => hdr!(self.agent.get(&url)).call(),
            "DELETE" => hdr!(self.agent.delete(&url)).call(),
            "POST" => hdr!(self.agent.post(&url)).send(body.as_bytes()),
            "PUT" => hdr!(self.agent.put(&url)).send(body.as_bytes()),
            _ => return Err(format!("metodo {method} non gestito")),
        };
        let mut resp = res.map_err(|e| format!("rete: {e}"))?;
        let status = resp.status().as_u16();
        let text = resp.body_mut().read_to_string().map_err(|e| format!("lettura risposta: {e}"))?;
        let json: Value = if text.trim().is_empty() { Value::Null } else { serde_json::from_str(&text).unwrap_or(Value::String(text)) };
        if status >= 400 {
            let msg = json["message"].as_str().map(str::to_string).unwrap_or_else(|| json.to_string());
            return Err(format!("{status}: {msg}"));
        }
        Ok(json)
    }
}

#[cfg(test)]
mod tests {
    use super::sign;

    // expected values computed independently with `printf '%s' '<string>' | sha1sum`
    #[test]
    fn signature_get_empty_body() {
        let s = sign("EgWIz07P0HYwtQDs", "MtSwSHyX7Y8E0DNzi6tz8hU6cn1h", "GET", "https://eu.api.ovh.com/1.0/vps", "", 1366560945);
        assert_eq!(s, "$1$7c1ae486f86be7419042c4cb93d8ce16abb87050");
    }

    #[test]
    fn signature_post_no_body() {
        let s = sign("secret", "ck", "POST", "https://eu.api.ovh.com/1.0/vps/vps-abc.vps.ovh.net/reboot", "", 1700000000);
        assert_eq!(s, "$1$e9d4393f8330d7aee5e9a6ec08b999c16c7b0757");
    }

    /// rete vera: chiavi finte, OVH deve rispondere con un errore di credenziali, non di firma o rete.
    /// `cargo test -- --ignored`
    #[test]
    #[ignore]
    fn live_rejects_fake_keys() {
        let c = super::Client::connect(super::Creds { app_key: "fake".into(), app_secret: "fake".into(), consumer_key: "fake".into() }).unwrap();
        let e = c.get("/vps").unwrap_err();
        assert!(e.starts_with("403") || e.starts_with("400"), "{e}");
        println!("{e}");
    }

    #[test]
    fn enc_ip_block() {
        assert_eq!(super::enc("203.0.113.10/32"), "203.0.113.10%2F32");
        assert_eq!(super::enc("2001:41d0::/64"), "2001%3A41d0%3A%3A%2F64");
    }

    #[test]
    fn signature_post_json_body() {
        let body = serde_json::json!({"description": "x"}).to_string();
        let s = sign("as", "ck", "POST", "https://eu.api.ovh.com/1.0/vps/v/createSnapshot", &body, 42);
        assert_eq!(s, "$1$a10814286096b8466a134df6fba4985b91937303");
    }
}
