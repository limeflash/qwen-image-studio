const { invoke, convertFileSrc } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const $ = (id) => document.getElementById(id);
const GB = (b) => (b / 1e9).toFixed(2);
const GBs = (mib) => (mib / 1024).toFixed(1);

const CHECK = '<svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M3.5 8.5l3 3 6-7"/></svg>';
const ARROW = '<svg width="10" height="10" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M8 3v10M4 9l4 4 4-4"/></svg>';

let S = null;
let copiedUntil = 0;
let copiedWhich = null;

/* Measured with SageAttention on a 16 GB card: 1024² ≈ 56 s at 20 steps, and the
   cost tracks pixel count. */
const SIZES = [
  { w: 1024, h: 1024, rw: 9, rh: 9 },
  { w: 1664, h: 928, rw: 12, rh: 7 },
  { w: 928, h: 1664, rw: 7, rh: 12 },
  { w: 2048, h: 2048, rw: 11, rh: 11 },
];
let size = SIZES[0];
let alpha = false;
const etaOf = (s) => Math.round((s.w * s.h) / (1024 * 1024) * 56);
const etaText = (sec) => (sec < 90 ? `${Math.round(sec / 5) * 5} s` : `${Math.round(sec / 60)} min`);

/* The panel re-renders every second. Writing identical innerHTML still restarts CSS
   animations and drops hover state, so each list keeps the last markup it drew. */
const memo = new Map();
function paint(el, html, wire) {
  if (memo.get(el.id) === html) return false;
  memo.set(el.id, html);
  el.innerHTML = html;
  if (wire) wire();
  return true;
}

/* The single blue element is whatever the app needs from you next. */
function nextAction(s) {
  if (s.screen === "install") {
    if (s.downloads.some((d) => d.state === "failed")) return "retry";
    if (s.downloads.length === 0 || s.downloads.every((d) => d.state === "pending")) return "download";
    return null;
  }
  if (s.status.kind === "error") return "start";
  if (s.loaded_tier && s.loaded_tier !== s.selected_tier) return "restart";
  if (s.status.kind === "off") return "start";
  if (s.tunnel.is_new) return "copy-tunnel";
  return null;
}

/* ----------------------------------------------------------------- heading */

function renderHead(s, blue) {
  const dot = $("dot"), word = $("word"), sub = $("subline"), btn = $("primary");
  sub.className = "subline";

  if (s.screen === "install") {
    dot.className = "dot hidden";
    const busy = s.downloads.length && !s.downloads.every((d) => d.state === "pending");
    const failed = s.downloads.filter((d) => d.state === "failed").length;
    const paused = s.downloads.some((d) => d.state === "paused");
    const got = s.downloads.reduce((a, d) => a + d.got, 0);
    const all = s.downloads.reduce((a, d) => a + d.total, 0);
    const pct = all ? Math.floor((got * 100) / all) : 0;

    word.textContent = busy ? "Installing" : "Install";
    if (!busy) {
      const short = s.free_bytes > 0 && s.free_bytes < s.needed_bytes;
      sub.textContent = `Downloads ${GB(s.needed_bytes)} GB. Pick a model tier — the rest is required.`;
      btn.hidden = false;
      btn.textContent = `Download ${GB(s.needed_bytes)} GB`;
      btn.className = "pill lg";
      btn.disabled = short;
      btn.onclick = short ? null : () => invoke("start_install");
    } else if (paused) {
      sub.textContent = `Paused at ${pct}% · ${GB(all - got)} GB left`;
      btn.hidden = false;
      btn.textContent = "Resume";
      btn.className = "pill lg";
      btn.onclick = () => invoke("pause_install", { paused: false });
    } else if (failed) {
      sub.innerHTML = `Stopped at ${pct}% · <span style="color:var(--danger)">${failed} file${failed > 1 ? "s" : ""} failed</span>`;
      btn.hidden = true;
    } else {
      sub.innerHTML = `<span class="lit">${pct}%</span> · ${GB(got)} of ${GB(all)} GB`;
      btn.hidden = false;
      btn.textContent = "Pause";
      btn.className = "pill lg ghost";
      btn.onclick = () => invoke("pause_install", { paused: true });
    }
    return;
  }

  const k = s.status.kind;
  dot.className = "dot " + (k === "stopping" ? "off" : k);
  word.textContent = k === "stopping" ? "Off" : k[0].toUpperCase() + k.slice(1);

  const tierOf = (id) => (s.tiers.find((t) => t.id === id) || {}).label || id;
  if (k === "error") {
    sub.className = "subline bad";
    sub.textContent = s.status.message;
  } else if (k === "loading") {
    sub.textContent = `Reading ${tierOf(s.selected_tier)} from disk · ${s.load_elapsed ?? 0} s`;
  } else if (k === "stopping") {
    sub.textContent = "Stopping…";
  } else if (k === "ready") {
    if (s.busy)
      sub.textContent =
        s.busy_eta && s.busy_elapsed != null
          ? `${s.busy} · ${s.busy_elapsed} s of about ${s.busy_eta}`
          : `${s.busy}…`;
    else if (s.last_oom) { sub.className = "subline bad"; sub.textContent = s.last_oom; }
    else if (s.loaded_tier !== s.selected_tier)
      sub.textContent = `Running ${tierOf(s.loaded_tier)} · ${tierOf(s.selected_tier)} selected — restart to switch`;
    else sub.textContent = `${tierOf(s.loaded_tier)} · loaded in ${s.last_load_secs ?? "?"} s`;
  } else {
    const hint = s.last_load_secs ? `loads in about ${s.last_load_secs} s` : "loads in about a minute";
    sub.textContent = `${tierOf(s.selected_tier)} selected · ${hint}`;
  }

  btn.hidden = false;
  if (blue === "restart") {
    btn.textContent = `Restart with ${tierOf(s.selected_tier)}`;
    btn.className = "pill lg";
    btn.onclick = () => invoke("restart_engine");
  } else if (k === "off" || k === "error") {
    btn.textContent = "Start";
    btn.className = "pill lg";
    btn.disabled = false;
    btn.onclick = () => invoke("start_engine");
  } else if (k === "stopping") {
    btn.textContent = "Stopping…";
    btn.className = "pill lg ghost";
    btn.disabled = true;
  } else {
    btn.textContent = "Stop";
    btn.className = "pill lg ghost";
    btn.disabled = false;
    btn.onclick = () => invoke("stop_engine");
  }
}

/* ----------------------------------------------------------------- install */

function renderInstall(s, blue) {
  const all = s.downloads.reduce((a, d) => a + d.total, 0) || 1;
  paint($("bar"), s.downloads
    .map((d) => {
      const w = ((d.total / all) * 836 - 2).toFixed(1);
      const f = d.total ? Math.min(1, d.got / d.total) : 0;
      const cls = d.state === "done" ? "done" : d.state === "stalled" ? "stalled" : d.state === "failed" ? "failed" : "";
      return `<i class="${cls}" style="width:${(w * f).toFixed(1)}px;min-width:0"></i><span style="width:${(w * (1 - f)).toFixed(1)}px"></span>`;
    })
    .join(""));

  let blueUsed = false;
  const rowsHtml = s.downloads
    .map((d) => {
      const done = d.state === "done";
      const glyph = done ? `<span class="glyph">${CHECK}</span>` : `<span class="glyph ${d.state}"><b></b></span>`;
      const size = done || d.state === "unpacking" || d.state === "verifying"
        ? `${GB(d.total)} GB`
        : `${GB(d.got)} / ${GB(d.total)} GB`;
      let retry = "";
      if (d.state === "failed") {
        const cls = !blueUsed && blue === "retry" ? "pill sm" : "pill sm ghost";
        blueUsed = true;
        retry = `<button class="${cls}" data-retry="${d.id}">Retry</button>`;
      }
      const note = d.note ? `<span class="rownote ${d.state === "failed" ? "failed" : ""}">${d.note}</span>` : "";
      return `<li>${glyph}<span class="rowname">${d.name}</span><span class="rowsize">${size}</span>${note}${retry}</li>`;
    })
    .join("");
  paint($("rows"), rowsHtml, () =>
    $("rows").querySelectorAll("[data-retry]").forEach((b) => {
      b.onclick = () => invoke("retry_download", { id: b.dataset.retry });
    })
  );

  const busy = s.downloads.length && !s.downloads.every((d) => d.state === "pending");
  $("tiers-install").hidden = busy;
  if (!busy) renderTiers($("tiers-install"), s, false);
  $("also").textContent = busy
    ? ""
    : "Also: text encoder 4.68 · vision projector 1.08 · VAE 0.63 · engine 0.33 · CUDA runtime 0.56 GB";
  const drive = (s.root.match(/^[A-Za-z]:/) || ["the disk"])[0];
  const tight = s.free_bytes > 0 && s.free_bytes < s.needed_bytes;
  const pathEl = $("path");
  pathEl.className = "foot" + (tight ? " bad" : "");
  paint(
    pathEl,
    busy
      ? ""
      : tight
        ? `Not enough space on ${drive} — ${GB(s.needed_bytes)} GB needed, ${GB(s.free_bytes)} GB free. <button class="link" id="change">Change</button>`
        : `to ${s.root}\models · ${GB(s.free_bytes)} GB free · <button class="link" id="change">Change</button>`,
    () => {
      const c = document.getElementById("change");
      if (c) c.onclick = () => invoke("change_root");
    }
  );
  // Four rows all saying "can't reach huggingface.co" state the fact but not the cure.
  const blocked = s.downloads.filter((d) => d.note.includes("can't reach"));
  $("hint").className = "foot" + (blocked.length ? " bad" : "");
  $("hint").textContent = blocked.length
    ? `${blocked[0].note.replace("failed · can't reach ", "")} is unreachable from this network. ` +
      "A VPN fixes it, or set HF_ENDPOINT to a proxy that mirrors it and reopen the app."
    : busy
      ? "Closing the window pauses the download. It picks up where it left off next time."
      : "";
}

/* ----------------------------------------------------------------- tiers */

function renderTiers(el, s, live) {
  const html = s.tiers
    .map((t) => {
      const sel = (t.id === s.selected_tier ? " sel" : "") + (t.fits ? "" : " unfit");
      const loaded = live && s.loaded_tier === t.id ? '<span class="loaded"></span>' : "";
      const size = `${t.on_disk ? "" : ARROW + " "}${GB(t.size)} GB`;
      const dis = live && s.status.kind === "loading" ? "disabled" : "";
      return `<button class="tier${sel}" data-tier="${t.id}" ${dis}>
        <span class="l1">${loaded}<span class="nm">${t.label}</span><span class="sz">${size}</span></span>
        <span class="l2">${t.desc}</span></button>`;
    })
    .join("");
  paint(el, html, () =>
    el.querySelectorAll("[data-tier]").forEach((b) => {
      b.onclick = () => invoke("select_tier", { id: b.dataset.tier });
    })
  );
}

/* ----------------------------------------------------------------- panel */

function renderGauge(s) {
  const total = s.vram_total || 16384;
  const px = 836 / total;
  const other = Math.max(0, s.vram_used - s.vram_engine);
  const eng = s.vram_engine;
  const freeMiB = total - s.vram_used;

  let cls = "safe", legendCls = "", tail = `free ${GBs(freeMiB)}`;
  if (s.status.kind === "loading") cls = "load";
  else if (freeMiB < 512) { cls = "crit"; legendCls = "crit"; tail = `${GBs(freeMiB)} GB free — generation will fail`; }
  else if (freeMiB < 1536) { cls = "warn"; legendCls = "warn"; tail = `${GBs(freeMiB)} GB free — tight for large images`; }

  $("gauge").querySelectorAll("i").forEach((n) => n.remove());
  const mk = (c, w) => {
    const i = document.createElement("i");
    i.className = c;
    i.style.width = Math.max(0, w * px - 2).toFixed(1) + "px";
    $("gauge").prepend(i);
    return i;
  };
  if (eng > 0) mk(cls, eng);
  mk("other", other);

  $("legend").className = "legend " + legendCls;
  $("legend").textContent = eng > 0
    ? `other ${GBs(other)} · engine ${GBs(eng)} · ${tail}`
    : `other ${GBs(other)} · ${tail}`;
  $("readout").className = "readout " + legendCls;
  $("readout").textContent = `${GBs(s.vram_used)} / ${Math.round(total / 1024)} GB`;

  const peak = $("peak");
  peak.hidden = !(s.vram_peak > s.vram_used && eng > 0);
  peak.style.left = (s.vram_peak * px).toFixed(1) + "px";
}

function paintUrl(el, url, secret) {
  if (!url) { el.textContent = ""; return; }
  const m = url.match(/^(https?:\/\/)([^/]+)\/([^/]+)\/mcp$/);
  if (!m) { el.textContent = url; return; }
  const [, scheme, host, sec] = m;
  const short = sec.length > 8 ? `${sec.slice(0, 4)}…${sec.slice(-4)}` : sec;
  el.innerHTML = `<span class="dim">${scheme}</span>${host}<span class="dim">/${short}</span><span class="mid">/mcp</span>`;
}

function renderConnect(s, blue) {
  paintUrl($("local-url"), s.local_url);
  $("local-url").onclick = () => copy(s.local_url, "local");

  const t = s.tunnel;
  const tu = $("tunnel-url"), btn = $("copy-tunnel"), tg = $("toggle"), note = $("notice");

  tg.className = "toggle" + (t.state === "off" ? "" : " on") + (t.state === "connecting" ? " busy" : "");
  tg.onclick = () => invoke("set_tunnel", { on: t.state === "off" });

  note.className = "notice";
  if (t.state === "off") {
    tu.className = "url mono idle";
    tu.textContent = "Off";
    btn.hidden = true;
    note.textContent = "Turn on the tunnel to reach this PC from your laptop or phone.";
  } else if (t.state === "connecting") {
    tu.className = "url mono idle";
    tu.textContent = "Connecting to Cloudflare…";
    btn.hidden = true;
    note.textContent = "";
  } else if (t.state === "failed") {
    tu.className = "url mono idle";
    tu.textContent = "Couldn't reach Cloudflare";
    btn.hidden = false;
    btn.className = "pill sm";
    btn.textContent = "Retry";
    btn.onclick = () => invoke("set_tunnel", { on: true });
    note.className = "notice bad";
    note.textContent = t.message || "Check the internet connection, then retry.";
  } else {
    tu.className = "url mono";
    paintUrl(tu, t.url);
    tu.onclick = () => copy(t.url, "tunnel");
    btn.hidden = false;
    btn.textContent = "Copy";
    btn.className = blue === "copy-tunnel" ? "pill sm" : "pill sm ghost";
    btn.onclick = () => copy(t.url, "tunnel");
    if (t.is_new) note.textContent = "New since last launch. claude.ai still has the old one — copy this into Settings → Connectors.";
    else if (t.connected_at) note.textContent = `claude.ai connected through the tunnel at ${t.connected_at}.`;
    else note.textContent = "";
  }

  if (Date.now() < copiedUntil) {
    const b = copiedWhich === "tunnel" ? btn : $("copy-local");
    if (!b.hidden) { b.className = "pill sm ghost"; b.innerHTML = CHECK + " Copied"; }
    if (copiedWhich === "tunnel" && t.state === "up")
      note.textContent = t.connected_at
        ? `claude.ai connected through the tunnel at ${t.connected_at}.`
        : "Copied. Waiting for claude.ai to connect through it…";
  }
}

async function copy(text, which) {
  if (!text) return;
  try { await navigator.clipboard.writeText(text); } catch { return; }
  copiedUntil = Date.now() + 1500;
  copiedWhich = which;
  if (which === "tunnel") invoke("mark_copied");
  render(S);
  setTimeout(() => render(S), 1600);
}

function renderGallery(s) {
  const g = $("gallery");
  const esc = (v) => String(v).replace(/&/g, "&amp;").replace(/"/g, "&quot;").replace(/</g, "&lt;");
  const ph = s.busy ? '<div class="ph"><b></b></div>' : "";
  const html =
    ph +
    s.gallery
      .map(
        (i) =>
          `<img src="${convertFileSrc(i.path)}" title="${esc(i.prompt)}

${i.width}×${i.height} · ${i.at} · click to open, right-click to copy the prompt" data-p="${esc(i.path)}" data-prompt="${esc(i.prompt)}">`
      )
      .join("");
  paint(g, html, () =>
    g.querySelectorAll("img").forEach((n) => {
      n.onclick = () => invoke("reveal", { path: n.dataset.p });
      n.oncontextmenu = (e) => {
        e.preventDefault();
        copyPrompt(n.dataset.prompt);
      };
    })
  );
  if (Date.now() >= promptCopiedUntil)
    $("recent").textContent = s.gallery.length || s.busy ? "Recent" : "Recent — nothing yet";
}

/* Confirmation lands in the label you are already looking at — no toast. */
let promptCopiedUntil = 0;
async function copyPrompt(text) {
  if (!text) return;
  try { await navigator.clipboard.writeText(text); } catch { return; }
  promptCopiedUntil = Date.now() + 1500;
  $("recent").textContent = "Prompt copied";
  setTimeout(() => { promptCopiedUntil = 0; render(S); }, 1600);
}

function renderSizes(s) {
  const ready = s.status.kind === "ready";
  const html =
    SIZES.map((z) => {
      const on = z === size ? " on" : "";
      const eta = z.w * z.h > 1024 * 1024 ? `<span class="eta">· ${etaText(etaOf(z))}</span>` : "";
      return `<button class="chip${on}" data-w="${z.w}" data-h="${z.h}">
        <span class="ratio" style="width:${z.rw}px;height:${z.rh}px"></span>${z.w} × ${z.h}${eta}</button>`;
    }).join("") +
    `<button class="chip alpha${alpha ? " on" : ""}" data-alpha="1" aria-pressed="${alpha}">
      <span class="ratio"></span>Transparent</button>`;

  paint($("sizes"), html, () => {
    $("sizes").querySelectorAll("[data-w]").forEach((b) => {
      b.onclick = () => {
        size = SIZES.find((z) => z.w === +b.dataset.w && z.h === +b.dataset.h) || SIZES[0];
        render(S);
      };
    });
    const a = $("sizes").querySelector("[data-alpha]");
    if (a) a.onclick = () => { alpha = !alpha; render(S); };
  });
  $("sizes").style.opacity = ready ? "1" : ".5";
  $("sizes").style.pointerEvents = ready ? "" : "none";
}

function renderPanel(s, blue) {
  renderGauge(s);
  renderTiers($("tiers-panel"), s, true);
  renderConnect(s, blue);

  const ready = s.status.kind === "ready" && !s.busy;
  const inp = $("prompt"), gen = $("generate");
  inp.disabled = !ready;
  inp.placeholder = s.status.kind === "ready" ? "Try a prompt" : "Start the engine to try a prompt";
  gen.hidden = s.status.kind !== "ready";
  gen.disabled = !ready;
  gen.textContent = s.busy ? "Generating…" : "Generate";

  renderSizes(s);
  renderGallery(s);
}

/* ----------------------------------------------------------------- driver */

function render(s) {
  try { render_(s); } catch (e) {
    const sub = $("subline");
    sub.className = "subline bad";
    sub.textContent = "UI error: " + (e && e.message ? e.message : e);
    invoke("ui_log", { msg: "render threw: " + (e && e.stack ? e.stack : e) });
  }
}

function render_(s) {
  if (!s) return;
  S = s;
  const blue = nextAction(s);
  $("screen-install").hidden = s.screen !== "install";
  $("screen-panel").hidden = s.screen !== "panel";
  renderHead(s, blue);
  if (s.screen === "install") renderInstall(s, blue);
  else renderPanel(s, blue);
}

$("copy-local").onclick = () => copy(S?.local_url, "local");
$("open-folder").onclick = () => invoke("reveal", { path: null });
$("generate").onclick = () => {
  const v = $("prompt").value.trim();
  if (!v) return;
  invoke("test_generate", { prompt: v, width: size.w, height: size.h, transparent: alpha });
  $("prompt").value = "";
};
$("prompt").onkeydown = (e) => { if (e.key === "Enter") $("generate").click(); };

listen("state", (e) => render(e.payload))
  .then(() => invoke("ui_log", { msg: "listener attached" }))
  .catch((e) => invoke("ui_log", { msg: "listen failed: " + e }));
invoke("get_state").then(render);
