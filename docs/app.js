// Data is generated from benchmarks/results.json by benchmarks/publish.py.
// DOM elements
const scenarioButtons = document.querySelectorAll("[data-scenario]");
const scenarioLabel = document.querySelector("[data-scenario-label]");
const runsLabel = document.querySelector("[data-runs]");
const speedupValue = document.querySelector("[data-speedup]");
const workloadSelect = document.querySelector('[data-workload]');
const workloadSource = document.querySelector('[data-workload-source]');
const benchmarkNote = document.querySelector('[data-benchmark-note]');
const benchmarkVersions = document.querySelector('[data-benchmark-versions]');
const speedupLabel = document.querySelector('[data-speedup-label]');
const themeToggle = document.querySelector(".theme-toggle");

// Formatters
const formatSeconds = (value) => `${value.toFixed(3)}s`;
const formatSpeed = (value) => `${value.toFixed(1)}x`;

// Selected benchmark
let currentScenario = "warm";
let currentWorkload = "components-500";

// ========================================
// Theme Management
// ========================================

function getSystemTheme() {
  return window.matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark";
}

function getStoredTheme() {
  try {
    return localStorage.getItem("theme");
  } catch {
    return null;
  }
}

function setStoredTheme(theme) {
  try {
    if (theme) {
      localStorage.setItem("theme", theme);
    } else {
      localStorage.removeItem("theme");
    }
  } catch {
    // localStorage not available
  }
}

function applyTheme(theme) {
  document.documentElement.setAttribute("data-theme", theme);
}

function initTheme() {
  const stored = getStoredTheme();
  // Use stored preference, otherwise follow system
  applyTheme(stored || getSystemTheme());
}

function toggleTheme() {
  const current = document.documentElement.getAttribute("data-theme");
  const newTheme = current === "dark" ? "light" : "dark";
  applyTheme(newTheme);
  setStoredTheme(newTheme);
}

// Listen for system theme changes - always follow system when it changes
window.matchMedia("(prefers-color-scheme: light)").addEventListener("change", () => {
  // Clear any stored preference and follow system
  setStoredTheme(null);
  applyTheme(getSystemTheme());
});

// ========================================
// Benchmark Visualization
// ========================================

function render(key) {
  const workload = BENCHMARK_WORKLOADS[currentWorkload];
  const data = workload?.scenarios[key];
  if (!data) return;
  currentScenario = key;

  scenarioButtons.forEach((button) => {
    const active = button.dataset.scenario === key;
    button.setAttribute("aria-selected", String(active));
    button.tabIndex = active ? 0 : -1;
  });
  if (scenarioLabel) scenarioLabel.textContent = data.label;
  if (runsLabel) runsLabel.textContent = `Median of ${workload.runs} runs`;
  if (workloadSource) workloadSource.textContent = workload.source;
  if (benchmarkNote) benchmarkNote.textContent = workload.note;
  document.querySelector('[data-benchmark-date]').textContent = workload.date;
  document.querySelector('[data-benchmark-env]').textContent = workload.environment;
  document.querySelector('[data-benchmark-checkers]').textContent = workload.checkers;
  if (benchmarkVersions) {
    const packages = workload.packages;
    benchmarkVersions.textContent = `Svelte ${packages.svelte}` +
      (packages["@sveltejs/kit"] ? ` · Kit ${packages["@sveltejs/kit"]}` : "") +
      ` · TypeScript ${packages.typescript} / ${packages["@typescript/native"]}`;
  }

  const tools = ["svelte", "tsgo", "rs"];
  const max = Math.max(...tools.map((tool) => data[tool] ?? 0));
  tools.forEach((tool) => {
    const bar = document.querySelector(`[data-bar="${tool}"]`);
    const row = bar?.closest(".bench-row");
    const fill = bar?.querySelector(".bench-bar-fill");
    const time = document.querySelector(`[data-time="${tool}"]`);
    const measured = Number.isFinite(data[tool]);
    row?.classList.toggle("unmeasured", !measured);
    if (fill) {
      fill.style.width = measured ? `${data[tool] / max * 100}%` : "0%";
    }
    if (time) {
      time.textContent = measured ? formatSeconds(data[tool]) : "Not measured";
      time.classList.add("visible");
    }
  });
  document.querySelector("#bench").dataset.qualified = String(!workload.showSpeedup);
  if (speedupValue) {
    speedupValue.textContent = workload.showSpeedup ? formatSpeed(data.tsgo / data.rs) : "Diagnostics differ";
    speedupValue.classList.add("visible");
  }
  if (speedupLabel) {
    speedupLabel.textContent = workload.showSpeedup
      ? "faster than TS7 + incremental"
      : "";
  }
}

// ========================================
// Copy to Clipboard
// ========================================

function setupCopyButtons() {
  document.querySelectorAll(".copy-btn").forEach((btn) => {
    btn.addEventListener("click", async (e) => {
      e.preventDefault();
      const codeBlock = btn.closest("[data-copy]");
      const text = codeBlock?.dataset.copy || codeBlock?.textContent?.trim();

      if (!text) return;

      try {
        await navigator.clipboard.writeText(text);
        btn.classList.add("copied");

        // Show checkmark briefly
        const originalSVG = btn.innerHTML;
        btn.innerHTML = `<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="20,6 9,17 4,12"></polyline></svg>`;

        setTimeout(() => {
          btn.classList.remove("copied");
          btn.innerHTML = originalSVG;
        }, 1500);
      } catch (err) {
        console.error("Failed to copy:", err);
      }
    });
  });
}

// ========================================
// Install OS Tabs
// ========================================

function setupInstallTabs() {
  const installTabs = document.querySelector(".install-tabs");
  if (!installTabs) return;

  const buttons = installTabs.querySelectorAll("[data-os]");
  const commands = document.querySelectorAll(".cmd[data-os]");

  buttons.forEach((button) => {
    button.addEventListener("click", () => {
      const os = button.dataset.os;

      // Update tab states
      buttons.forEach((btn) => {
        btn.setAttribute("aria-selected", btn.dataset.os === os ? "true" : "false");
      });

      // Show/hide commands
      commands.forEach((cmd) => {
        cmd.hidden = cmd.dataset.os !== os;
      });
    });
  });
}

// ========================================
// Event Listeners
// ========================================

// Scenario tab clicks
scenarioButtons.forEach((button) => {
  button.addEventListener("click", () => {
    const scenario = button.dataset.scenario;
    if (scenario !== currentScenario) {
      render(scenario);
    }
  });
});

workloadSelect?.addEventListener("change", () => {
  currentWorkload = workloadSelect.value;
  render(currentScenario);
});

// Theme toggle
themeToggle?.addEventListener("click", toggleTheme);

// Keyboard navigation for tabs
document.querySelector(".bench-tabs")?.addEventListener("keydown", (e) => {
  const tabs = Array.from(scenarioButtons);
  const currentIndex = tabs.findIndex((t) => t.getAttribute("aria-selected") === "true");

  let newIndex = currentIndex;

  if (e.key === "ArrowRight" || e.key === "ArrowDown") {
    newIndex = (currentIndex + 1) % tabs.length;
  } else if (e.key === "ArrowLeft" || e.key === "ArrowUp") {
    newIndex = (currentIndex - 1 + tabs.length) % tabs.length;
  } else if (e.key === "Home") {
    newIndex = 0;
  } else if (e.key === "End") {
    newIndex = tabs.length - 1;
  } else {
    return;
  }

  e.preventDefault();
  tabs[newIndex].click();
  tabs[newIndex].focus();
});

// ========================================
// Initialize
// ========================================

document.addEventListener("DOMContentLoaded", () => {
  // Initialize theme
  initTheme();

  // Setup interactions
  setupCopyButtons();
  setupInstallTabs();

  if (workloadSelect) {
    workloadSelect.replaceChildren();
    ["components-500", "components-100", "careswitch-web"].forEach((key) => {
      const workload = BENCHMARK_WORKLOADS[key];
      if (!workload) return;
      const option = document.createElement("option");
      option.value = key;
      option.textContent = workload.title;
      workloadSelect.append(option);
    });
    workloadSelect.value = currentWorkload;
  }

  // Initial render
  render("warm");
});
