# Ovhelm

A Windows desktop app for managing OVHcloud VPS and dedicated servers through the OVHcloud API, without opening the web control panel. Written in Rust with egui; one 6 MB exe, no installer, no SSH.

**Ovhelm is a personal project. It is not an OVHcloud product and has no connection with OVHcloud**, which does not support it and is not responsible for it. It talks to the public OVHcloud API with your own keys. The look follows the new OVHcloud console (colors from their ODS design system, published under Apache 2.0) so the screens feel familiar; the logo is different.

I built it because checking whether my VPS was frozen meant logging into the manager every time. Now I open the app, see CPU and RAM of every server, and Windows tells me when one stops answering.

## What it does

- Dashboard you put together yourself: pin CPU, RAM, network, status or backups of any server, plus alerts, upcoming renewals and recent invoices. The tiles fill the window, no scrolling.
- VPS: details and renewal, reboot / start / stop, KVM console in the browser, CPU / RAM / network charts (1 hour, 24 hours, 7 days), automated backup restore points, snapshots, recent tasks, OS reinstall.
- Dedicated servers: details, hardware and disks, bandwidth and traffic (MRTG), boot and rescue mode, IPMI HTML5 console, OVH monitoring on/off, recent tasks, OS reinstall with hostname and SSH key.
- Network and IP: every IP block, reverse DNS, the OVH network firewall with its 20 rules, anti-DDoS status.
- Account: every service with expiry date and renewal mode, last year's invoices (PDF opens in the browser), account SSH keys.
- Tray: closing the window keeps Ovhelm in the tray. Every minute it checks all machines and sends a Windows notification when one goes down, stops sending metrics for more than 7 minutes (likely frozen) or comes back. "Quit" in the tray menu really closes it.
- Seven languages: English, Italiano, Français, Deutsch, Español, Polski, Português. The flag button top right switches them; the first start follows the Windows language.

Anything that destroys data (restoring a backup or snapshot, reinstalling) asks twice and wants the server name typed in. Everything else asks once.

## Build

```
cargo build --release
```

The exe is `target\release\ovhelm.exe`. `cargo test` checks the request signature against vectors computed separately with `sha1sum`, date handling, URL encoding, and that every language file has every key and placeholder. `cargo test -- --ignored` also runs two live checks: fake keys against OVHcloud (must return `403: This application key is invalid`) and reading the metrics of your first VPS with the keys the app saved (GET only).

## Credentials

Ovhelm uses an application key, application secret and consumer key (the `$1$` + SHA1 signature, with the time taken from `/auth/time`). I skipped OAuth2 service accounts: they need an IAM policy and a token that expires every hour.

1. On the first screen, click "Create them on OVHcloud". It opens `https://eu.api.ovh.com/createToken/?GET=/*&POST=/*&PUT=/*&DELETE=/*`.
2. Log in, name the application, pick a validity ("Unlimited" means it never expires) and confirm.
3. Paste the three keys into Ovhelm and press "Save and connect".

That token covers the whole API, so you create it once and it keeps working as new sections are added. It also means anyone holding your keys can do anything on the account, orders and terminations included. Ovhelm never calls those endpoints, but treat the keys like your OVHcloud password.

If you prefer a narrower token, create it by hand with these rules: `GET/POST/PUT/DELETE` on `/vps/*`, `GET /vps`, `GET/POST/PUT` on `/dedicated/server/*`, `GET /dedicated/server`, `GET/POST/PUT/DELETE` on `/ip/*`, `GET /ip`, `GET` on `/service`, `/service/*`, `/me/bill`, `/me/bill/*`, `/me`, and `GET/POST/DELETE` on `/me/sshKey` and `/me/sshKey/*`. Sections it cannot read say so.

The keys live in the Windows Credential Manager (generic credential `Ovhelm`). Never in a file, never in the repo. "Sign out" in the account menu (bottom left) deletes them. Pins and the chosen language are in `%APPDATA%\Ovhelm`.

## VPS metrics

On recent VPS models (tested on a VPS-2 2027) `/vps/{serviceName}/monitoring` returns 500 and `/vps/{serviceName}/status` returns `403: not implemented`. The real numbers live in Mimir (Prometheus): `GET /vps/{serviceName}/metricsToken` gives an endpoint and a token, and every query has to filter on `service_name="<serviceName>"` or Mimir rejects it. The token is reused for 10 minutes, because asking for a new one on every chart gets requests refused.

Series used: `rec_libvirtd_domain_cpu_usage`, `rec_libvirtd_domain_balloon_rss` and `balloon_current`, `rec_libvirtd_domain_net_rx_bytes` and `net_tx_bytes`.

RAM is what the VM takes on the host, guest page cache included, so it reads high even on an idle server. Good for spotting spikes and drops, not for knowing what your processes use. A gap in a line means OVHcloud received nothing: the VPS was off, rebooting or frozen.

## Performance

Idle: 0% CPU. With the animated banner on screen: about 7% of one core, at 30 fps, only while the window has focus, and never if Windows "animation effects" are off. V-sync is off and frames are capped at 60 per second by hand; with v-sync on, the GPU driver busy-waits for the monitor and an early build sat at 54% of a core doing nothing.

## Limits

- Live-tested on one VPS only. Dedicated servers, firewall rules and IP features follow the OVHcloud OpenAPI schema but I could not try them on a real account. If something breaks, open an issue with the error text.
- The IPMI console does not send `ipToAllow`. If OVHcloud requires it for your server, the error shows up bottom right.
- I do not know how `cpu_usage` scales with several vCores (whether it can go over 100); it is drawn as a percentage.
- Backup restore is always `full`.
- European endpoint only (`https://eu.api.ovh.com/1.0`).
- Windows only: Credential Manager, tray, notifications and the window frame are Win32.
- `--pos X,Y` opens the window at that position without taking focus (handy on a second monitor).

## Translations

One file per language in `lang/`, lines like `vps.riavvia = Reboot`. `{name}` is a placeholder and must stay as it is. Italian (`it.lang`) is the source. A missing key falls back to English, then Italian.

## License

MIT, see `LICENSE`. Source Sans 3 in `assets/fonts` is under the SIL Open Font License (`assets/fonts/OFL.md`).
