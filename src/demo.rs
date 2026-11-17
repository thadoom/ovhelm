//! `--demo`: dati inventati al posto delle API, per screenshot e prove senza un account.
//! IP dei range riservati alla documentazione (203.0.113.0/24, 198.51.100.0/24, 2001:db8::/32).
use serde_json::{json, Value};

pub const VPS: [&str; 2] = ["vps-1a2b3c4d.vps.ovh.net", "vps-5e6f7a8b.vps.ovh.net"];
pub const DEDI: &str = "ns3141592.ip-203-0-113.eu";
pub const IPS: [&str; 4] = ["203.0.113.10/32", "2001:db8:1::/64", "198.51.100.20/32", "203.0.113.42/32"];

fn date(now: i64, days: i64) -> String {
    crate::account::iso_date(now + days * 86400)
}

/// Serie finta: base + onda + rumore, con un buco (riavvio) se `gap`.
fn series(n: usize, base: f64, amp: f64, seed: f64, gap: bool) -> Vec<Value> {
    (0..n)
        .map(|i| {
            let t = i as f64 / n as f64;
            if gap && (0.62..0.66).contains(&t) {
                return Value::Null;
            }
            let noise = ((i as f64 * 12.9898 + seed).sin() * 43758.5453).fract().abs();
            json!((base + amp * (t * 9.0 + seed).sin().abs() * (0.6 + 0.4 * noise)).max(0.0))
        })
        .collect()
}

fn metrics(i: usize, period: &str) -> Value {
    let (_, step) = crate::vps::range_of(period);
    let n = match period {
        "1h" => 121,
        "lastweek" => 169,
        _ => 289,
    };
    let s = i as f64 * 3.0;
    let (cpu, ram) = if i == 0 { (18.0, 5200.0) } else { (4.0, 1900.0) };
    json!({ "step": step, "rows": [
        {"label": "CPU", "unit": "%", "used": series(n, cpu, 22.0, s, i == 0), "max": 100.0, "age": 20},
        {"label": "RAM", "unit": "MB", "used": series(n, ram, 900.0, s + 1.0, i == 0), "max": if i == 0 { 8000.0 } else { 4000.0 }, "age": 20},
        {"label": "Rete in", "unit": "KB/s", "used": series(n, 40.0, 420.0, s + 2.0, i == 0), "max": null, "age": 20},
        {"label": "Rete out", "unit": "KB/s", "used": series(n, 25.0, 160.0, s + 3.0, i == 0), "max": null, "age": 20},
    ]})
}

/// Dato finto per una chiave dello store; None = niente (resta lo spinner).
pub fn get(key: &str, now: i64) -> Option<Value> {
    let parts: Vec<&str> = key.split('/').collect();
    let vi = |s: &str| VPS.iter().position(|v| *v == s).unwrap_or(0);
    Some(match parts.as_slice() {
        ["list", "vps"] => json!(VPS),
        ["list", "dedi"] => json!([DEDI]),
        ["me"] => json!({"firstname": "Alex", "name": "Martin", "nichandle": "am12345-ovh"}),
        ["vps", s, "info"] => {
            let i = vi(s);
            let (name, offer, vcore, mem, disk, ip) = if i == 0 { ("web-prod", "VPS-2 2027", 4, 8192, 75, 10) } else { ("staging", "VPS-1 2027", 2, 4096, 40, 20) };
            let ipv4 = if i == 0 { format!("203.0.113.{ip}") } else { format!("198.51.100.{ip}") };
            json!({
                "vps": {"name": s, "displayName": name, "state": "running", "vcore": vcore, "memoryLimit": mem, "zone": "Region OpenStack: os-gra",
                        "model": {"offer": offer, "name": offer, "vcore": vcore, "memory": mem, "disk": disk}},
                "svc": {"expiration": date(now, if i == 0 { 212 } else { 9 }), "renew": {"automatic": i == 0, "period": 1}, "engagedUpTo": if i == 0 { Value::from(date(now, 212)) } else { Value::Null }},
                "ips": [ipv4, format!("2001:db8:{}::1", i + 1)],
                "dc": {"longName": if i == 0 { "Gravelines GRA11" } else { "Strasbourg SBG5" }},
                "os": {"name": if i == 0 { "Debian 13" } else { "Ubuntu 24.04" }},
            })
        }
        ["vps", s, "m", period] => metrics(vi(s), period),
        ["vps", s, "backup"] => {
            let pts: Vec<String> = (0..7).map(|d| format!("{}T03:12:00+01:00", date(now, -d))).collect();
            json!({"conf": {"state": if vi(s) == 0 { "enabled" } else { "disabled" }, "schedule": "03:12:00"}, "points": if vi(s) == 0 { pts } else { vec![] }})
        }
        ["vps", s, "snap"] => json!({"enabled": vi(s) == 0, "snap": if vi(s) == 0 { json!({"creationDate": format!("{}T18:40:00+01:00", date(now, -3)), "description": "before upgrade"}) } else { Value::Null }}),
        ["vps", _, "tasks"] => json!([
            {"type": "rebootVm", "state": "done", "date": format!("{}T09:15:00+01:00", date(now, 0)), "progress": 100},
            {"type": "createSnapshot", "state": "done", "date": format!("{}T18:40:00+01:00", date(now, -3)), "progress": 100},
            {"type": "reinstallVm", "state": "done", "date": format!("{}T11:02:00+01:00", date(now, -21)), "progress": 100},
        ]),
        ["dedi", _, "info"] => json!({
            "srv": {"name": DEDI, "state": "ok", "powerState": "poweron", "commercialRange": "Advance-1", "datacenter": "gra3", "rack": "G112A",
                    "ip": "203.0.113.77", "reverse": "db-01.example.com", "os": "debian12_64", "monitoring": true, "bootId": 1, "iam": {"displayName": "db-01"}},
            "svc": {"expiration": date(now, 41)},
            "net": {"bandwidth": {"OvhToInternet": {"unit": "Mbps", "value": 1000}, "InternetToOvh": {"unit": "Mbps", "value": 1000}, "OvhToOvh": {"unit": "Gbps", "value": 25}},
                    "connection": {"unit": "Gbps", "value": 1}, "vrack": {"bandwidth": {"unit": "Mbps", "value": 1000}},
                    "routing": {"ipv4": {"ip": "203.0.113.77/24", "gateway": "203.0.113.254"}, "ipv6": {"ip": "2001:db8:77::/64", "gateway": "2001:db8:77::ff"}},
                    "traffic": {"outputQuotaUsed": {"unit": "TB", "value": 3.4}, "outputQuotaSize": {"unit": "TB", "value": 10}, "isThrottled": false}},
        }),
        ["ip", "list"] => json!([
            {"ip": IPS[0], "type": "vps", "routedTo": {"serviceName": VPS[0]}},
            {"ip": IPS[1], "type": "vps", "routedTo": {"serviceName": VPS[0]}},
            {"ip": IPS[2], "type": "vps", "routedTo": {"serviceName": VPS[1]}},
            {"ip": IPS[3], "type": "failover", "routedTo": {"serviceName": DEDI}},
        ]),
        ["ip", ..] if key.ends_with("/rev") => json!([{"ipReverse": "203.0.113.10", "reverse": "web.example.com."}]),
        ["ip", ..] if key.ends_with("/fw") => json!([{"ipOnFirewall": "203.0.113.10", "enabled": true, "state": "ok"}]),
        ["ip", ..] if key.contains("/rules/") => json!([
            {"sequence": 0, "action": "permit", "rule": "permit tcp from any to 203.0.113.10/32 port 22"},
            {"sequence": 1, "action": "permit", "rule": "permit tcp from any to 203.0.113.10/32 port 443"},
            {"sequence": 2, "action": "permit", "rule": "permit tcp established"},
            {"sequence": 19, "action": "deny", "rule": "deny ipv4 from any to 203.0.113.10/32"},
        ]),
        ["ip", ..] if key.ends_with("/mit") => json!([]),
        ["svc", "all"] => json!([
            {"resource": {"displayName": "staging", "name": VPS[1]}, "route": {"path": "/vps/{serviceName}"}, "expirationDate": date(now, 9), "renew": {"mode": "manual"}, "state": "ok"},
            {"resource": {"displayName": "db-01", "name": DEDI}, "route": {"path": "/dedicated/server/{serviceName}"}, "expirationDate": date(now, 41), "renew": {"mode": "automaticV2024", "interval": "P1M"}, "state": "ok"},
            {"resource": {"displayName": "example.com", "name": "example.com"}, "route": {"path": "/domain/{serviceName}"}, "expirationDate": date(now, 120), "renew": {"mode": "automaticV2024", "interval": "P1Y"}, "state": "ok"},
            {"resource": {"displayName": "web-prod", "name": VPS[0]}, "route": {"path": "/vps/{serviceName}"}, "expirationDate": date(now, 212), "renew": {"mode": "automaticV2024", "interval": "P1M"}, "state": "ok"},
        ]),
        ["bills"] => json!((0..6).map(|m| json!({"billId": format!("FR{}", 4100230 + m), "date": format!("{}T06:51:00+01:00", date(now, -30 * m)),
            "priceWithTax": {"text": format!("€ {:.2}", 38.4 + (m % 3) as f64 * 11.0)}})).collect::<Vec<_>>()),
        _ => return None,
    })
}

/// Avvisi finti per la dashboard.
pub fn alerts() -> Vec<crate::watch::Alert> {
    vec![
        crate::watch::Alert { when: "09:16".into(), machine: VPS[0].into(), text: crate::i18n::tr("watch.di_nuovo_ok").into(), bad: false },
        crate::watch::Alert { when: "09:11".into(), machine: VPS[0].into(), text: crate::i18n::fill(crate::i18n::tr("watch.accesa_ma_nessuna_metrica_da_2"), &[("0", "7".into())]), bad: true },
    ]
}
