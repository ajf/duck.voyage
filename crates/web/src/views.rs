//! Maud views: HTML as compile-time-checked markup. Engineering-grade UI —
//! correct and legible, no styling ambitions beyond that.

use auth::ProviderSummary;
use domain::DuckCode;
use jiff::Timestamp;
use maud::{html, Markup, PreEscaped, DOCTYPE};
use storage::{
    CommentView, DuckSummary, FlockDuckStatus, FollowedDuck, MySighting, NotificationView,
    SightingView, VesselOption,
};

use crate::version::BuildInfo;

const CSS: &str = r#"
:root { color-scheme: light dark; }
body { font-family: system-ui, sans-serif; margin: 0 auto; max-width: 760px;
       padding: 0 1rem 4rem; line-height: 1.5; }
header { display: flex; gap: 1rem; align-items: baseline; padding: 1rem 0;
         border-bottom: 1px solid #8884; margin-bottom: 1.5rem; flex-wrap: wrap; }
header .spacer { flex: 1; }
h1 { font-size: 1.4rem; margin: 0; }
h2 { font-size: 1.1rem; margin-top: 2rem; }
a { color: inherit; }
table { border-collapse: collapse; width: 100%; }
td, th { text-align: left; padding: 0.3rem 0.6rem 0.3rem 0; border-bottom: 1px solid #8883; }
form.stack { display: grid; gap: 0.6rem; max-width: 26rem; margin: 0.5rem 0 1rem; }
form.inline { display: inline; }
input, textarea, select, button { font: inherit; padding: 0.35rem 0.5rem; }
button { cursor: pointer; }
img.photo { max-width: 100%; height: auto; border-radius: 4px; }
.muted { opacity: 0.65; font-size: 0.9rem; }
.card { border: 1px solid #8884; border-radius: 6px; padding: 0.8rem 1rem; margin: 0.8rem 0; }
.code { font-family: ui-monospace, monospace; letter-spacing: 0.08em; }
.badge { background: #d33; color: #fff; border-radius: 999px; padding: 0 0.5em; font-size: 0.8rem; }
.flash { background: #2a72; border: 1px solid #2a7; border-radius: 6px; padding: 0.5rem 1rem; margin: 0.8rem 0; }
footer { margin-top: 4rem; padding-top: 0.8rem; border-top: 1px solid #8883;
         font-size: 0.78rem; opacity: 0.55; display: flex; gap: 0.6rem;
         justify-content: center; flex-wrap: wrap; }
footer .code { letter-spacing: normal; }
"#;

/// What the chrome needs to know about the viewer.
pub struct Nav {
    pub display_name: Option<String>,
    pub logged_in: bool,
    pub unread: i64,
}

impl Nav {
    pub fn anonymous() -> Self {
        Self { display_name: None, logged_in: false, unread: 0 }
    }
}

pub struct Page;

impl Page {
    fn layout(title: &str, nav: &Nav, flash: Option<&str>, body: Markup) -> Markup {
        html! {
            (DOCTYPE)
            html lang="en" {
                head {
                    meta charset="utf-8";
                    meta name="viewport" content="width=device-width, initial-scale=1";
                    title { (title) " · duck.voyage" }
                    style { (PreEscaped(CSS)) }
                    script src="/static/htmx.min.js" defer {}
                }
                body hx-boost="true" {
                    header {
                        h1 { a href="/" style="text-decoration:none" { "🦆 duck.voyage" } }
                        a href="/missing" { "missing ducks" }
                        span class="spacer" {}
                        @if nav.logged_in {
                            a href="/me/flocks" { "my flocks" }
                            a href="/me" {
                                (nav.display_name.as_deref().unwrap_or("me"))
                                @if nav.unread > 0 { " " span class="badge" { (nav.unread) } }
                            }
                            form class="inline" method="post" action="/logout" {
                                button { "log out" }
                            }
                        } @else {
                            a href="/login" { "log in" }
                        }
                    }
                    @if let Some(message) = flash {
                        div class="flash" { (message) }
                    }
                    main { (body) }
                    footer {
                        span { "duck-tracker" }
                        span class="code" {
                            (BuildInfo::VERSION)
                            @if let Some(sha) = BuildInfo::distinct_sha() { " (" (sha) ")" }
                        }
                        a href="https://github.com/ajf/duck.voyage" hx-boost="false" { "source" }
                    }
                }
            }
        }
    }

    fn summary_table(ducks: &[DuckSummary]) -> Markup {
        html! {
            @if ducks.is_empty() {
                p class="muted" { "Nothing here yet." }
            } @else {
                table {
                    tr { th { "duck" } th { "last seen" } th { "aboard" } th { "finds" } }
                    @for d in ducks {
                        tr {
                            td {
                                a class="code" href={ "/d/" (d.code.as_str()) } {
                                    (d.code.display_grouped())
                                }
                                @if let Some(name) = &d.name { " · " (name) }
                            }
                            td { (When::ago(&d.last_seen_at)) }
                            td { (d.last_vessel_name) }
                            td { (d.sighting_count) }
                        }
                    }
                }
            }
        }
    }

    pub fn front(nav: &Nav, recent: &[DuckSummary], most: &[DuckSummary]) -> Markup {
        Self::layout("home", nav, None, html! {
            p {
                "Found a rubber duck with a QR code on a cruise? Scanning it leads here. "
                "This is the registry of traveling ducks: where they started, where they've been."
            }
            h2 { "Recently found" }
            (Self::summary_table(recent))
            h2 { "Most traveled" }
            (Self::summary_table(most))
            h2 { "Set your own ducks loose" }
            p class="muted" {
                "Grab a flock of codes under " a href="/me/flocks" { "my flocks" }
                ", print the labels, stick them on ducks, and hide them aboard."
            }
        })
    }

    pub fn missing(nav: &Nav, ducks: &[DuckSummary]) -> Markup {
        Self::layout("missing ducks", nav, None, html! {
            p {
                "Ducks that were found at least once, then went silent for over a year. "
                "Keep an eye out — every one of them is still out there somewhere."
            }
            (Self::summary_table(ducks))
        })
    }

    pub fn login(nav: &Nav, providers: &[ProviderSummary], return_to: Option<&str>) -> Markup {
        Self::layout("log in", nav, None, html! {
            p { "Log in to record finds, comment, and follow ducks." }
            @for p in providers {
                p {
                    a href={
                        "/login/" (p.slug)
                        @if let Some(rt) = return_to { "?return_to=" (rt) }
                    } {
                        "Continue with " (p.display_name)
                    }
                }
            }
            @if providers.is_empty() {
                p class="muted" { "No login providers are configured." }
            }
        })
    }

    pub fn error_page(code: &str, message: &str) -> Markup {
        Self::layout(code, &Nav::anonymous(), None, html! {
            h2 { (code) }
            p { (message) }
        })
    }

    pub fn origination_form(nav: &Nav, code: &DuckCode) -> Markup {
        Self::layout("define duck", nav, None, html! {
            h2 { "Define duck " span class="code" { (code.display_grouped()) } }
            p {
                "This code is yours and hasn't been brought to life yet. Snap a photo of the "
                "duck and tell the world what it is — you can do this at home, before it "
                "travels. It stays invisible to everyone else until you set it sailing."
            }
            form class="stack" method="post" enctype="multipart/form-data"
                 action={ "/d/" (code.as_str()) "/originate" } {
                label { "Photo of the duck (required)"
                    input type="file" name="photo" accept="image/*" required;
                }
                label { "Description (required)"
                    textarea name="description" rows="3" required
                        placeholder="A tiny pirate duck with an eyepatch" {}
                }
                label { "Name (optional)"
                    input type="text" name="name" placeholder="Captain Quackbeard";
                }
                label {
                    input type="checkbox" name="set_sail" value="on";
                    " it's already in place — set sail immediately"
                }
                button { "Save the duck" }
            }
            p class="muted" {
                "Left unchecked, the duck is staged: scan its sticker (or open this page) "
                "once it's hidden aboard and press \u{201c}Set sail\u{201d}."
            }
        })
    }

    /// The owner's view of a staged duck: details preview + the set-sail
    /// confirmation. Everyone else sees a 404.
    pub fn staged(nav: &Nav, code: &DuckCode, details: &domain::DuckDetails) -> Markup {
        Self::layout("ready to sail", nav, None, html! {
            h2 {
                span class="code" { (code.display_grouped()) }
                @if let Some(name) = &details.name { " · " (name.as_str()) }
                " — staged"
            }
            img class="photo" src={ "/d/" (code.as_str()) "/photo" } alt="photo of this duck";
            p { (details.description.as_str()) }
            p class="muted" {
                "Defined " (When::ago(&details.defined_at))
                ". Only you can see this page until it sets sail."
            }
            form class="stack" method="post" action={ "/d/" (code.as_str()) "/set-sail" } {
                button { "⛵ Set sail — the duck is in place" }
            }
            p class="muted" {
                a href={ "/d/" (code.as_str()) "/qr.png" } hx-boost="false" { "QR label" }
                " · scanning the printed sticker leads right back here."
            }
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn duck(
        nav: &Nav,
        flash: Option<&str>,
        code: &DuckCode,
        name: Option<&str>,
        description: &str,
        originated_at: &Timestamp,
        is_owner: bool,
        is_following: bool,
        sightings: &[SightingView],
        comments: &[CommentView],
        vessels: &[VesselOption],
    ) -> Markup {
        let title = name.map(str::to_owned).unwrap_or_else(|| code.display_grouped());
        Self::layout(&title, nav, flash, html! {
            h2 {
                span class="code" { (code.display_grouped()) }
                @if let Some(name) = name { " · " (name) }
            }
            img class="photo" src={ "/d/" (code.as_str()) "/photo" }
                alt="origin photo of this duck";
            p { (description) }
            p class="muted" {
                "Traveling since " (When::date(originated_at))
                " · " (sightings.len()) " find" @if sightings.len() != 1 { "s" }
                @if is_owner {
                    " · yours — "
                    a href={ "/d/" (code.as_str()) "/qr.png" } hx-boost="false" { "QR label" }
                }
            }

            @if nav.logged_in {
                form class="inline" method="post"
                     action={ "/d/" (code.as_str()) @if is_following { "/unfollow" } @else { "/follow" } } {
                    button { @if is_following { "unfollow" } @else { "follow this duck" } }
                }
            }

            h2 { "Log a find" }
            @if nav.logged_in {
                form class="stack" method="post" enctype="multipart/form-data"
                     action={ "/d/" (code.as_str()) "/sightings" } {
                    label { "Which ship?"
                        select name="vessel_id" required {
                            option value="" disabled selected { "pick a vessel" }
                            (Self::vessel_options(vessels))
                        }
                    }
                    label { "When?"
                        input type="datetime-local" name="seen_at" required;
                    }
                    label { "Note (optional)"
                        textarea name="note" rows="2"
                            placeholder="Perched above the towel animals on deck 9" {}
                    }
                    label { "Photo (optional)"
                        input type="file" name="photo" accept="image/*";
                    }
                    input type="hidden" name="latitude" id="geo-lat";
                    input type="hidden" name="longitude" id="geo-lon";
                    div {
                        button type="button" onclick="duckAttachLocation()" {
                            "📍 attach my location"
                        }
                        " " span id="geo-status" class="muted" {}
                    }
                    button { "Record the find" }
                }
                script { (PreEscaped(GEO_JS)) }
            } @else {
                p {
                    a href={ "/login?return_to=/d/" (code.as_str()) } {
                        "Log in to record your find"
                    }
                }
            }

            h2 { "Sighting history" }
            @if sightings.is_empty() {
                p class="muted" { "Not spotted yet. It's out there somewhere." }
            }
            @for s in sightings {
                div class="card" {
                    strong { (s.vessel_name) }
                    " · " (When::ago(&s.seen_at))
                    @if let Some(by) = &s.by_display_name { " · found by " (by) }
                    @if let Some(c) = &s.coordinates {
                        " · "
                        a href=(format!(
                            "https://www.openstreetmap.org/?mlat={:.6}&mlon={:.6}#map=8/{:.4}/{:.4}",
                            c.latitude(), c.longitude(), c.latitude(), c.longitude()
                        )) hx-boost="false" { "📍 " (c) }
                    }
                    @if let Some(note) = &s.note { p { (note) } }
                    @if s.has_photo {
                        img class="photo"
                            src={ "/d/" (code.as_str()) "/sightings/" (s.id.get()) "/photo" }
                            alt="sighting photo" loading="lazy";
                    }
                }
            }

            h2 { "Comments" }
            @for c in comments {
                div class="card" {
                    span class="muted" {
                        (c.by_display_name.as_deref().unwrap_or("someone"))
                        " · " (When::ago(&c.created_at))
                    }
                    p { (c.body) }
                }
            }
            @if nav.logged_in {
                form class="stack" method="post" action={ "/d/" (code.as_str()) "/comments" } {
                    textarea name="body" rows="2" required placeholder="Say something nice" {}
                    button { "Comment" }
                }
            } @else if comments.is_empty() {
                p class="muted" { "No comments yet." }
            }
        })
    }

    pub fn me(
        nav: &Nav,
        notifications: &[NotificationView],
        sightings: &[MySighting],
        follows: &[FollowedDuck],
    ) -> Markup {
        Self::layout("me", nav, None, html! {
            h2 { "Activity" }
            @if notifications.is_empty() {
                p class="muted" { "Nothing yet. Follow a duck and you'll hear when it's found." }
            } @else {
                @if nav.unread > 0 {
                    form class="inline" method="post" action="/me/notifications/read" {
                        button { "mark all read" }
                    }
                }
                @for n in notifications {
                    div class="card" {
                        @if n.unread { span class="badge" { "new" } " " }
                        a class="code" href={ "/d/" (n.duck_code.as_str()) } {
                            (n.duck_code.display_grouped())
                        }
                        @if let Some(name) = &n.duck_name { " (" (name) ")" }
                        " was found aboard " strong { (n.vessel_name) }
                        " · " (When::ago(&n.seen_at))
                    }
                }
            }

            h2 { "Your finds" }
            @if sightings.is_empty() {
                p class="muted" { "No finds logged yet." }
            } @else {
                table {
                    tr { th { "duck" } th { "aboard" } th { "when" } }
                    @for s in sightings {
                        tr {
                            td {
                                a class="code" href={ "/d/" (s.duck_code.as_str()) } {
                                    (s.duck_code.display_grouped())
                                }
                                @if let Some(name) = &s.duck_name { " · " (name) }
                            }
                            td { (s.vessel_name) }
                            td { (When::ago(&s.seen_at)) }
                        }
                    }
                }
            }

            h2 { "Ducks you follow" }
            @if follows.is_empty() {
                p class="muted" { "None yet." }
            } @else {
                table {
                    tr { th { "duck" } th { "following since" } }
                    @for f in follows {
                        tr {
                            td {
                                a class="code" href={ "/d/" (f.duck_code.as_str()) } {
                                    (f.duck_code.display_grouped())
                                }
                                @if let Some(name) = &f.duck_name { " · " (name) }
                            }
                            td { (When::date(&f.followed_at)) }
                        }
                    }
                }
            }
        })
    }

    pub fn flocks(
        nav: &Nav,
        flash: Option<&str>,
        flocks: &[(domain::Flock, Vec<FlockDuckStatus>)],
        mint_batch_max: u16,
    ) -> Markup {
        Self::layout("my flocks", nav, flash, html! {
            p {
                "A flock is your own block of duck codes under one prefix. Mint codes, print "
                "the labels, stick them on ducks, then scan each sticker to bring its duck to life."
            }
            form class="stack" method="post" action="/flocks" {
                label { "Label (optional, just for you)"
                    input type="text" name="label" placeholder="Alaska trip 2026";
                }
                button { "Grab a new flock" }
            }
            @for (flock, ducks) in flocks {
                div class="card" {
                    h2 style="margin-top:0" {
                        span class="code" { (flock.code.as_str()) "-" }
                        @if let Some(label) = &flock.label { " · " (label.as_str()) }
                    }
                    p class="muted" {
                        (ducks.len()) " duck" @if ducks.len() != 1 { "s" }
                        " · since " (When::date(&flock.created_at))
                    }
                    form class="inline" method="post"
                         action={ "/flocks/" (flock.id.get()) "/ducks" } {
                        input type="number" name="count" value="10" min="1"
                              max=(mint_batch_max) style="width:5rem";
                        button { "mint codes" }
                    }
                    @if !ducks.is_empty() {
                        table {
                            tr { th { "code" } th { "status" } th { "finds" } th {} }
                            @for status in ducks {
                                tr {
                                    td {
                                        a class="code" href={ "/d/" (status.duck.code.as_str()) } {
                                            (status.duck.code.display_grouped())
                                        }
                                    }
                                    td {
                                        @match &status.duck.lifecycle {
                                            domain::DuckLifecycle::Allocated => {
                                                a href={ "/d/" (status.duck.code.as_str()) } {
                                                    "allocated — define it"
                                                }
                                            }
                                            domain::DuckLifecycle::Staged(_) => {
                                                a href={ "/d/" (status.duck.code.as_str()) } {
                                                    "staged — ready to set sail"
                                                }
                                            }
                                            domain::DuckLifecycle::Sailing { .. } => { "⛵ sailing" }
                                        }
                                    }
                                    td { (status.sighting_count) }
                                    td {
                                        a href={ "/d/" (status.duck.code.as_str()) "/qr.png" }
                                          hx-boost="false" { "QR" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        })
    }
}

/// Browser geolocation for the sighting form: fills the hidden lat/lon
/// fields on request. Pure enhancement — without JS (or permission) the form
/// submits without coordinates.
const GEO_JS: &str = r#"
function duckAttachLocation() {
  var status = document.getElementById("geo-status");
  if (!navigator.geolocation) { status.textContent = "geolocation unavailable"; return; }
  status.textContent = "locating…";
  navigator.geolocation.getCurrentPosition(function (p) {
    document.getElementById("geo-lat").value = p.coords.latitude.toFixed(6);
    document.getElementById("geo-lon").value = p.coords.longitude.toFixed(6);
    status.textContent = "location attached ✓";
  }, function () { status.textContent = "location unavailable"; });
}
"#;

impl Page {
    /// The vessel picker's options, grouped by current cruise line;
    /// operator-less vessels fall into a trailing "Other" group.
    fn vessel_options(vessels: &[VesselOption]) -> Markup {
        let mut by_line: std::collections::BTreeMap<&str, Vec<&VesselOption>> =
            std::collections::BTreeMap::new();
        let mut unaffiliated: Vec<&VesselOption> = Vec::new();
        vessels.iter().for_each(|v| match v.line.as_deref() {
            Some(line) => by_line.entry(line).or_default().push(v),
            None => unaffiliated.push(v),
        });
        html! {
            @for (line, ships) in &by_line {
                optgroup label=(line) {
                    @for v in ships {
                        option value=(v.id.get()) { (v.name) }
                    }
                }
            }
            @if !unaffiliated.is_empty() {
                optgroup label="Other" {
                    @for v in &unaffiliated {
                        option value=(v.id.get()) { (v.name) }
                    }
                }
            }
        }
    }
}

/// Human time rendering.
pub struct When;

impl When {
    pub fn date(ts: &Timestamp) -> String {
        ts.strftime("%Y-%m-%d").to_string()
    }

    pub fn ago(ts: &Timestamp) -> String {
        let seconds = (Timestamp::now().as_second() - ts.as_second()).max(0);
        let (n, unit) = match seconds {
            s if s < 3600 => return "just now".into(),
            s if s < 86_400 => (s / 3600, "hour"),
            s if s < 86_400 * 60 => (s / 86_400, "day"),
            s if s < 86_400 * 365 * 2 => (s / (86_400 * 30), "month"),
            s => (s / (86_400 * 365), "year"),
        };
        format!("{n} {unit}{} ago", if n == 1 { "" } else { "s" })
    }
}

/// Standalone error page (no session context needed).
pub fn error_page(code: &str, message: &str) -> Markup {
    Page::error_page(code, message)
}
