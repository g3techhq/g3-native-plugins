//! The test bed's stylesheet.
//!
//! Kept apart from the markup because it is long and never interesting when
//! reading what the checks actually do.

pub const STYLE: &str = r#"
:root {
  color-scheme: light dark;
  --bg: #ffffff;
  --fg: #16181d;
  --muted: #5d6470;
  --line: #e3e6eb;
  --card: #f7f8fa;
  --pass: #0f7b3f;
  --pass-bg: #e4f5ea;
  --fail: #b3261e;
  --fail-bg: #fce8e6;
  --wait: #8a5a00;
  --wait-bg: #fdf0d5;
  --idle: #6b7280;
  --idle-bg: #eef0f3;
}
@media (prefers-color-scheme: dark) {
  :root {
    --bg: #14161a;
    --fg: #e8eaed;
    --muted: #9aa2ae;
    --line: #2a2e35;
    --card: #1c1f25;
    --pass: #6ddc95;
    --pass-bg: #12301f;
    --fail: #ff9d95;
    --fail-bg: #3a1614;
    --wait: #f0c469;
    --wait-bg: #33260c;
    --idle: #9aa2ae;
    --idle-bg: #23272e;
  }
}

* { box-sizing: border-box; }
body { margin: 0; background: var(--bg); color: var(--fg);
       font: 15px/1.45 system-ui, -apple-system, sans-serif; }
main { padding: 16px 16px 40px; max-width: 720px; margin: 0 auto; }

header { position: sticky; top: 0; z-index: 5; background: var(--bg);
         padding: 12px 0 10px; border-bottom: 1px solid var(--line); }
h1 { font-size: 19px; margin: 0; letter-spacing: -.01em; }
.sub { margin: 3px 0 0; font-size: 12.5px; color: var(--muted); }

.summary { display: flex; gap: 6px; flex-wrap: wrap; margin-top: 10px; }
.tally { font-size: 12px; font-weight: 600; padding: 4px 9px; border-radius: 999px; }
.tally.pass { color: var(--pass); background: var(--pass-bg); }
.tally.fail { color: var(--fail); background: var(--fail-bg); }
.tally.waiting { color: var(--wait); background: var(--wait-bg); }
.tally.idle { color: var(--idle); background: var(--idle-bg); }

section { margin-top: 18px; }
.section-head { display: flex; align-items: baseline; justify-content: space-between;
                gap: 8px; margin-bottom: 8px; }
h2 { font-size: 12px; margin: 0; text-transform: uppercase;
     letter-spacing: .08em; color: var(--muted); }
.what { font-size: 12px; color: var(--muted); text-align: right; }

.card { background: var(--card); border: 1px solid var(--line);
        border-radius: 12px; padding: 12px; }

.check { padding: 7px 0; border-bottom: 1px solid var(--line); }
.check:last-child { border-bottom: 0; }
.check-head { display: flex; align-items: center; justify-content: space-between; gap: 10px; }
.check-name { font-size: 13.5px; font-weight: 550; }
.check-detail { margin-top: 3px; font-size: 12px; color: var(--muted);
                font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
                word-break: break-word; }

.badge { flex: 0 0 auto; font-size: 11px; font-weight: 700; letter-spacing: .04em;
         text-transform: uppercase; padding: 3px 8px; border-radius: 999px; }
.badge.pass { color: var(--pass); background: var(--pass-bg); }
.badge.fail { color: var(--fail); background: var(--fail-bg); }
.badge.waiting { color: var(--wait); background: var(--wait-bg); }
.badge.idle { color: var(--idle); background: var(--idle-bg); }

.preview-wrap { display: none; margin-top: 10px; }
.preview-wrap.on { display: block; }
.preview { width: 100%; aspect-ratio: 4 / 3; border-radius: 10px;
           background: #000; object-fit: cover; display: block; }

.row { display: flex; flex-wrap: wrap; gap: 7px; margin-top: 10px; }
button { flex: 0 0 auto; padding: 9px 13px; font-size: 13.5px; font-weight: 550;
         border-radius: 9px; border: 1px solid var(--line);
         background: var(--bg); color: inherit; }
button:active { background: var(--idle-bg); }

.hint { margin: 8px 0 0; font-size: 11.5px; color: var(--muted);
        font-family: ui-monospace, SFMono-Regular, Menlo, monospace; }

details { margin-top: 22px; }
summary { font-size: 12px; text-transform: uppercase; letter-spacing: .08em;
          color: var(--muted); cursor: pointer; }
.log { margin: 8px 0 0; padding: 11px; border-radius: 10px; font-size: 11.5px;
       line-height: 1.5; white-space: pre-wrap; word-break: break-word;
       background: var(--card); border: 1px solid var(--line);
       max-height: 45vh; overflow: auto;
       font-family: ui-monospace, SFMono-Regular, Menlo, monospace; }
"#;
