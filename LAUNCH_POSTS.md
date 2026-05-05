# Launch Posts — Drafts

These are ready-to-use posts for the platforms that move the needle for SEO ranking. Each one creates a high-authority backlink to https://mhostai.com.

## Why these matter for SEO

Google's algorithm weighs domain authority and backlink quality heavily. A new domain (mhostai.com is brand new) can have perfect technical SEO and still rank low for 3-6 months without backlinks. Posting on these platforms creates:

1. **Direct backlinks** to mhostai.com (each link counts as a "vote")
2. **Branded search volume** — when someone Googles "mhost" after seeing the post, that's a ranking signal
3. **Traffic** — actual humans visiting and engaging with the site
4. **Indexation speed** — Googlebot crawls these platforms constantly, so it discovers your site faster

Order to post (best results):

1. **GitHub repo polish first** (already done — README has prominent mhostai.com link)
2. **Show HN** (HackerNews) — biggest single backlink boost if it hits the front page
3. **ProductHunt** — keep it for a Tuesday/Wednesday launch (most traffic)
4. **Reddit** — same day or day after HN
5. **dev.to** article — week after launch
6. **Awesome lists** PRs — anytime

---

## 1. Show HN (HackerNews)

**Submit URL:** https://news.ycombinator.com/submit
**Title (≤80 chars):**

> Show HN: Mhost – PM2 alternative in Rust with built-in AI agent

**URL field:** `https://mhostai.com`

**Text (optional but recommended for Show HN — keep it concise & honest):**

```
Hi HN — I built mhost because I got tired of PM2's quirks (memory leaks, no real
health checks, every notification requiring a plugin) and wanted to see how far
a Rust rewrite could go.

What's there now:
- Single 14MB binary, zero runtime deps. Drop-in replacement for `pm2 start`,
  `pm2 logs`, `pm2 reload`, etc.
- HTTP / TCP / script health probes (not just "is the PID alive")
- Built-in AI agent that diagnoses crashes, suggests fixes, and (if you opt in)
  runs a fix-and-verify loop. Bring your own OpenAI/Claude key.
- Self-healing brain with playbooks — e.g. "if memory > 90% three times, restart"
- Telegram / Slack / Discord / PagerDuty / Email / Teams / Webhook notifications
  out of the box, no plugin
- Reverse proxy with auto-TLS
- Cloud fleet manager (optional, paid SaaS) — manage processes across boxes

The CLI is MIT and free forever. The cloud dashboard is the paid tier.

Would love feedback — especially from anyone running PM2 in prod and missing
something specific.

GitHub: https://github.com/maqalaqil/mhost
Site:   https://mhostai.com
```

**Posting tips:**
- Best time: Tuesday-Thursday, 8-10 AM Pacific
- DO NOT mass-message friends to upvote (HN auto-detects ring voting and shadow-bans)
- DO answer every comment in the first 2 hours — engagement keeps you on the front page

---

## 2. ProductHunt

**Submit URL:** https://www.producthunt.com/products/new

- **Name:** mhost
- **Tagline (≤60 chars):** AI-powered PM2 alternative in Rust
- **Topics:** Developer Tools, DevOps, Open Source, Productivity, Artificial Intelligence
- **Website:** https://mhostai.com
- **Maker:** @maqalaqil

**Description (~260 chars):**

> mhost is a modern PM2 alternative built in Rust. Single 14MB binary with a built-in AI agent, self-healing brain, real health probes, multi-channel alerts, and a cloud fleet dashboard. The CLI is open source (MIT), the cloud is the paid tier.

**Gallery:**
- og-image.svg as main image
- 3-4 screenshots (TUI dashboard, fleet map, AI diagnose terminal, web dashboard)
- 30-second screen recording showing `mhost start ecosystem.toml`

**Comments to seed (post these yourself within first hour):**
- "Why I built it" — the same backstory from HN
- "How it differs from systemd / supervisor / forever"
- "Roadmap"

**Posting tips:**
- Tuesday or Wednesday only
- Post at 12:01 AM Pacific time exactly
- Ask 5-10 friends in advance to "Notify me" — they'll see the launch when they wake up

---

## 3. Reddit

### r/rust

**Title:** I built mhost — a PM2 alternative in Rust with a built-in AI agent (MIT)

```
Hey r/rust 👋

I've been working on mhost for a while and just open-sourced it — a process
manager for Node/Python/Bun/Deno/anything-with-a-PID, written in Rust.

The Rust-y interesting bits:
- 16-crate workspace (core / daemon / cli / ipc / health / logs / notify /
  metrics / proxy / deploy / tui / ai / cloud / bot / api / config)
- Daemon ↔ CLI over Unix socket using a JSON-RPC IPC crate I wrote
- sled for state persistence, ratatui for the TUI dashboard
- axum for the public REST API + WebSocket streaming
- 14MB single binary — vs Node's ~50MB+ runtime alone

I'd love feedback on the architecture and any rough edges you hit.

GitHub: https://github.com/maqalaqil/mhost
Site:   https://mhostai.com
```

### r/devops

**Title:** Open-sourced mhost — modern PM2 replacement with real health probes and an AI ops agent

```
We've been running PM2 for years and the things that bothered us most were:

- "alive/dead" health checks that don't actually mean the app is healthy
- having to glue together notification scripts for every channel
- no real fleet view across multiple boxes

mhost is a Rust rewrite that ships:

- HTTP/TCP/script health probes
- Built-in Telegram, Slack, Discord, PagerDuty, Email, Teams, Webhook notifications
- An AI agent that diagnoses crashes from logs and suggests/runs fixes
- A self-healing "brain" with playbooks (e.g. "memory > 90% three times → restart")
- An optional managed cloud dashboard for multi-server fleets

CLI is MIT and free forever. Fleet dashboard is the paid SaaS.

Site: https://mhostai.com
Repo: https://github.com/maqalaqil/mhost

Would genuinely love to hear what would convince an existing PM2/systemd/Docker
shop to try it.
```

### r/selfhosted, r/node, r/sysadmin

Adapt the above. Lead with the use case for each subreddit.

---

## 4. dev.to / Hashnode / Medium article

**Title:** Why I Replaced PM2 with a Rust-Based Process Manager

This article already exists at `/blog/replacing-pm2-with-rust` on the site —
post it (or a slightly shorter version) on dev.to with a "canonical_url"
pointing back to mhostai.com so you don't get a duplicate-content penalty:

```yaml
---
title: "Why I Replaced PM2 with a Rust-Based Process Manager"
canonical_url: "https://mhostai.com/blog/replacing-pm2-with-rust"
cover_image: "https://mhostai.com/og-image.svg"
tags: rust, devops, pm2, ai
---
```

The `canonical_url` line is critical — without it, dev.to outranks your own site
for your own content.

---

## 5. Awesome Lists (10 minutes each)

Submit a PR adding mhost to:

- https://github.com/rust-unofficial/awesome-rust  (Application > Process management)
- https://github.com/sindresorhus/awesome-nodejs   (Tools > Process management)
- https://github.com/kahun/awesome-sysadmin
- https://github.com/Kickball/awesome-selfhosted   (Software > Monitoring or Software > DevOps)

These are slow-burn but every one creates a permanent backlink from a
high-authority repo.

---

## 6. Search engine direct submission (do this TODAY)

1. **Google Search Console** — https://search.google.com/search-console
   - Add property `mhostai.com`
   - Verify via DNS TXT record (most reliable)
   - Submit sitemap: `https://mhostai.com/sitemap.xml`
   - Use **URL Inspection** → "Request Indexing" for `/`, `/pricing`, `/pm2-alternative`, `/vs-pm2`, `/rust-process-manager`, `/ai-devops-agent`, `/blog/replacing-pm2-with-rust`

2. **Bing Webmaster Tools** — https://www.bing.com/webmasters
   - Same flow. Submit sitemap.
   - Bing has "IndexNow" — instant indexing API. Worth turning on.

3. **DuckDuckGo** — uses Bing's index, so #2 covers it.

4. **Yandex Webmaster** — https://webmaster.yandex.com (small but free).

---

## 7. After launch — measuring

| Metric | Where | What's good |
|---|---|---|
| Indexed pages | Google: `site:mhostai.com` | All sitemap URLs within 1 week |
| GSC clicks | Search Console > Performance | First clicks within 2 weeks |
| Position | GSC > Performance > Queries | Tracking "mhost", "pm2 alternative" |
| Backlinks | https://ahrefs.com/backlink-checker (free for first batch) | 10+ within first month |
| Direct traffic | Plausible (already installed) | Spikes after each launch post |
