// Aether Playground — main.js
// Handles wasm initialisation, button clicks, and output rendering.
// All wasm calls are synchronous once the module is loaded.

import init, { aether_check, aether_run, aether_format, aether_introspect }
  from "./pkg/aether_wasm.js";

// ── starter program (examples/02_refinement.ae) ───────────────────────────

const STARTER = `## Refinement types: \`min\` carries a postcondition the solver proves.
##
## The annotation \`result <= a && result <= b\` is checked at compile time by
## case-splitting on the \`if\` and discharging each branch via the linear-
## arithmetic decision procedure.

fn min(a: Int, b: Int) -> Int
  where result <= a && result <= b
  effects {} {
  if a <= b then a else b
}

## Without refinement: simple recursion that adds 1 each call.
fn inc(x: Int) -> Int effects {} {
  x + 1
}

fn main() -> Unit effects {IO} {
  let m = min(3, 7)
  print(str(m))
}
`;

// ── DOM refs ──────────────────────────────────────────────────────────────

const editor      = document.getElementById("editor");
const output      = document.getElementById("output");
const badge       = document.getElementById("status-badge");
const btnCheck    = document.getElementById("btn-check");
const btnRun      = document.getElementById("btn-run");
const btnFmt      = document.getElementById("btn-fmt");
const btnIntrospect = document.getElementById("btn-introspect");
const toggleVerbose = document.getElementById("toggle-verbose");

// ── init ──────────────────────────────────────────────────────────────────

editor.value = STARTER;

let wasmReady = false;

setBadge("running", "loading…");
setOutput(`<span class="spinner"></span> Loading Aether wasm module…`);

init().then(() => {
  wasmReady = true;
  setOutput(`<span class="out-hint">Wasm loaded. Press <strong>Run</strong> to execute, <strong>Check</strong> to type-check.</span>`);
  clearBadge();
}).catch(err => {
  setOutput(`<span class="out-error">Failed to load wasm module:\n${escHtml(String(err))}</span>\n\n<span class="out-hint">Make sure you ran <code>./build.sh</code> from the playground directory and are serving via HTTP (not file://).</span>`);
  setBadge("error", "error");
});

// ── button handlers ───────────────────────────────────────────────────────

btnRun.addEventListener("click", () => {
  if (!wasmReady) return;
  const src = editor.value;
  setBadge("running", "running…");
  // Yield to paint, then run.
  setTimeout(() => {
    try {
      const result = aether_run(src);
      renderRun(result);
    } catch(e) {
      renderFatalError(e);
    }
  }, 0);
});

btnCheck.addEventListener("click", () => {
  if (!wasmReady) return;
  const src = editor.value;
  setBadge("running", "checking…");
  setTimeout(() => {
    try {
      const result = aether_check(src);
      renderCheck(result);
    } catch(e) {
      renderFatalError(e);
    }
  }, 0);
});

btnFmt.addEventListener("click", () => {
  if (!wasmReady) return;
  const src = editor.value;
  const verbose = toggleVerbose.checked;
  try {
    const formatted = aether_format(src, verbose);
    if (formatted.startsWith("error:")) {
      setOutput(`<span class="out-error">${escHtml(formatted)}</span>`);
      setBadge("error", "error");
    } else {
      editor.value = formatted;
      setOutput(`<span class="out-ok">✓ formatted (${verbose ? "verbose" : "compact"})</span>`);
      setBadge("ok", "ok");
    }
  } catch(e) {
    renderFatalError(e);
  }
});

btnIntrospect.addEventListener("click", () => {
  if (!wasmReady) return;
  const src = editor.value;
  try {
    const result = aether_introspect(src);
    if (result.startsWith("error:")) {
      setOutput(`<span class="out-error">${escHtml(result)}</span>`);
      setBadge("error", "error");
    } else {
      setOutput(
        `<div class="out-section-label">module surface</div>` +
        `<span class="out-stdout">${escHtml(result)}</span>`
      );
      setBadge("ok", "ok");
    }
  } catch(e) {
    renderFatalError(e);
  }
});

// Keyboard shortcut: Ctrl+Enter / Cmd+Enter → Run
editor.addEventListener("keydown", e => {
  if ((e.ctrlKey || e.metaKey) && e.key === "Enter") {
    e.preventDefault();
    btnRun.click();
  }
  // Tab inserts two spaces instead of losing focus.
  if (e.key === "Tab") {
    e.preventDefault();
    const start = editor.selectionStart;
    const end   = editor.selectionEnd;
    editor.value = editor.value.slice(0, start) + "  " + editor.value.slice(end);
    editor.selectionStart = editor.selectionEnd = start + 2;
  }
});

// ── renderers ─────────────────────────────────────────────────────────────

function renderRun(result) {
  let html = "";

  if (result.stdout && result.stdout.length > 0) {
    html += `<div class="out-section-label">stdout</div>`;
    html += `<div class="out-stdout">${escHtml(result.stdout)}</div>`;
  }

  if (result.value != null) {
    html += `<div class="out-value">${escHtml(result.value)}</div>`;
  }

  if (result.error) {
    html += `<div class="out-section-label">error</div>`;
    html += `<div class="out-error">${escHtml(result.error)}</div>`;
  }

  if (result.ok && !result.stdout && result.value == null) {
    html += `<span class="out-ok">✓ main() returned Unit</span>`;
  }

  if (!html) {
    html = `<span class="out-hint">(no output)</span>`;
  }

  setOutput(html);
  setBadge(result.ok ? "ok" : "error", result.ok ? "ok" : "error");
}

function renderCheck(result) {
  let html = "";
  let badgeKind = "ok";
  let badgeText = "ok";

  if (result.errors.length > 0) {
    badgeKind = "error";
    badgeText = `${result.errors.length} error${result.errors.length !== 1 ? "s" : ""}`;
    html += `<div class="out-section-label">errors</div>`;
    html += renderDiags(result.errors, "error");
  }

  if (result.warnings.length > 0) {
    if (badgeKind !== "error") { badgeKind = "warning"; badgeText = `${result.warnings.length} warning${result.warnings.length !== 1 ? "s" : ""}`; }
    html += `<div class="out-section-label">warnings</div>`;
    html += renderDiags(result.warnings, "warning");
  }

  if (result.notes.length > 0) {
    html += `<div class="out-section-label">notes</div>`;
    html += renderDiags(result.notes, "note");
  }

  if (result.ok) {
    html += `<span class="out-ok">✓ types, effects, and refinements verified</span>`;
    if (result.warnings.length > 0) {
      html += `\n<span class="out-hint">(with ${result.warnings.length} warning${result.warnings.length !== 1 ? "s" : ""})</span>`;
    }
  }

  setOutput(html || `<span class="out-hint">(no diagnostics)</span>`);
  setBadge(badgeKind, badgeText);
}

function renderDiags(diags, cls) {
  return diags.map(d =>
    `<div class="diag ${cls}">` +
      `<span class="diag-loc">${d.line}:${d.col}</span>` +
      `<span class="diag-msg">${escHtml(d.message)}</span>` +
    `</div>`
  ).join("");
}

function renderFatalError(err) {
  setOutput(`<span class="out-error">Fatal JS error:\n${escHtml(String(err))}</span>`);
  setBadge("error", "error");
}

// ── helpers ───────────────────────────────────────────────────────────────

function setOutput(html) {
  output.innerHTML = html;
}

function setBadge(kind, text) {
  badge.className = `badge ${kind}`;
  badge.textContent = text;
}

function clearBadge() {
  badge.className = "badge";
  badge.textContent = "";
}

function escHtml(s) {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}
