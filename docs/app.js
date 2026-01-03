// Benchmark data
const BENCHMARKS = {
  cold: {
    label: "Cold start",
    runs: 3,
    svelte: 39.63,
    rs: 17.53,
    speedup: 2.26,
  },
  warm: {
    label: "Warm cache",
    runs: 5,
    svelte: 39.44,
    rs: 1.29,
    speedup: 30.50,
  },
  iterative: {
    label: "Iterative change",
    runs: 3,
    svelte: 39.80,
    rs: 2.50,
    speedup: 15.89,
  },
};

// DOM elements
const scenarioButtons = document.querySelectorAll("[data-scenario]");
const scenarioLabel = document.querySelector("[data-scenario-label]");
const runsLabel = document.querySelector("[data-runs]");
const speedupValue = document.querySelector("[data-speedup]");
const barSvelte = document.querySelector('[data-bar="svelte"]');
const barRs = document.querySelector('[data-bar="rs"]');
const timeSvelte = document.querySelector('[data-time="svelte"]');
const timeRs = document.querySelector('[data-time="rs"]');
const modal = document.getElementById("methodology-modal");
const themeToggle = document.querySelector(".theme-toggle");

// Formatters
const formatSeconds = (value) => `${value.toFixed(2)}s`;
const formatSpeed = (value) => `${value.toFixed(1)}x`;

// Animation state
let currentScenario = "warm";
let animationTimeouts = [];

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

function animateValue(element, start, end, duration, formatter) {
  const startTime = performance.now();

  function update(currentTime) {
    const elapsed = currentTime - startTime;
    const progress = Math.min(elapsed / duration, 1);

    // Ease out cubic
    const eased = 1 - Math.pow(1 - progress, 3);
    const current = start + (end - start) * eased;

    element.textContent = formatter(current);

    if (progress < 1) {
      requestAnimationFrame(update);
    }
  }

  requestAnimationFrame(update);
}

function clearAnimations() {
  animationTimeouts.forEach(clearTimeout);
  animationTimeouts = [];
}

function scheduleTimeout(fn, delay) {
  const id = setTimeout(fn, delay);
  animationTimeouts.push(id);
  return id;
}

function render(key, animate = true) {
  const data = BENCHMARKS[key];
  if (!data) return;

  // Clear any pending animations
  clearAnimations();

  currentScenario = key;

  // Update tab states
  scenarioButtons.forEach((button) => {
    const isActive = button.dataset.scenario === key;
    button.setAttribute("aria-selected", isActive ? "true" : "false");
  });

  // Update labels
  if (scenarioLabel) scenarioLabel.textContent = data.label;
  if (runsLabel) runsLabel.textContent = `n=${data.runs}`;

  // Calculate bar widths
  const max = Math.max(data.svelte, data.rs);
  const svelteWidth = (data.svelte / max) * 100;
  const rsWidth = (data.rs / max) * 100;

  // Get bar fill elements
  const svelteFill = barSvelte?.querySelector(".bench-bar-fill");
  const rsFill = barRs?.querySelector(".bench-bar-fill");

  if (animate) {
    // Disable transitions for instant reset
    svelteFill?.style.setProperty("transition", "none");
    rsFill?.style.setProperty("transition", "none");

    // Reset to zero
    if (svelteFill) svelteFill.style.width = "0%";
    if (rsFill) rsFill.style.width = "0%";
    timeSvelte?.classList.remove("visible");
    timeRs?.classList.remove("visible");
    speedupValue?.classList.remove("visible");

    // Set initial values while hidden
    if (timeSvelte) timeSvelte.textContent = "0.00s";
    if (timeRs) timeRs.textContent = "0.00s";
    if (speedupValue) speedupValue.textContent = "1.0x";

    // Force reflow to apply the reset
    void svelteFill?.offsetWidth;
    void rsFill?.offsetWidth;

    // Re-enable transitions by removing inline style
    svelteFill?.style.removeProperty("transition");
    rsFill?.style.removeProperty("transition");

    // Another reflow to ensure transition is active
    void svelteFill?.offsetWidth;

    // Start all animations together
    if (svelteFill) svelteFill.style.width = `${svelteWidth}%`;
    if (rsFill) rsFill.style.width = `${rsWidth}%`;

    // Show numbers immediately and animate values in sync with bars
    timeSvelte?.classList.add("visible");
    timeRs?.classList.add("visible");
    speedupValue?.classList.add("visible");
    if (timeSvelte) animateValue(timeSvelte, 0, data.svelte, 400, formatSeconds);
    if (timeRs) animateValue(timeRs, 0, data.rs, 400, formatSeconds);
    if (speedupValue) animateValue(speedupValue, 1, data.speedup, 400, formatSpeed);
  } else {
    // No animation - set values immediately
    if (svelteFill) svelteFill.style.width = `${svelteWidth}%`;
    if (rsFill) rsFill.style.width = `${rsWidth}%`;
    if (timeSvelte) {
      timeSvelte.textContent = formatSeconds(data.svelte);
      timeSvelte.classList.add("visible");
    }
    if (timeRs) {
      timeRs.textContent = formatSeconds(data.rs);
      timeRs.classList.add("visible");
    }
    if (speedupValue) {
      speedupValue.textContent = formatSpeed(data.speedup);
      speedupValue.classList.add("visible");
    }
  }
}

// Fill results table
function fillTable() {
  document.querySelectorAll("[data-bench]").forEach((el) => {
    const [scenario, field] = el.dataset.bench.split(":");
    const data = BENCHMARKS[scenario];
    if (!data) return;

    if (field === "svelte") el.textContent = formatSeconds(data.svelte);
    if (field === "rs") el.textContent = formatSeconds(data.rs);
    if (field === "speed") el.textContent = formatSpeed(data.speedup);
  });
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
// Modal
// ========================================

function openModal() {
  if (modal) {
    modal.showModal();
    document.body.style.overflow = "hidden";
  }
}

function closeModal() {
  if (modal) {
    modal.close();
    document.body.style.overflow = "";
  }
}

function setupModal() {
  // Open modal buttons
  document.querySelectorAll("[data-open-modal]").forEach((btn) => {
    btn.addEventListener("click", openModal);
  });

  // Close modal buttons
  document.querySelectorAll("[data-close-modal]").forEach((btn) => {
    btn.addEventListener("click", closeModal);
  });

  // Close on backdrop click
  modal?.addEventListener("click", (e) => {
    if (e.target === modal) {
      closeModal();
    }
  });

  // Close on Escape key
  modal?.addEventListener("keydown", (e) => {
    if (e.key === "Escape") {
      closeModal();
    }
  });
}

// ========================================
// Scroll Animations
// ========================================

function setupScrollAnimations() {
  const observer = new IntersectionObserver(
    (entries) => {
      entries.forEach((entry) => {
        if (entry.isIntersecting && entry.target.classList.contains("hero-bench")) {
          render(currentScenario, true);
          observer.unobserve(entry.target);
        }
      });
    },
    { threshold: 0.3 }
  );

  const heroBench = document.querySelector(".hero-bench");
  if (heroBench) {
    observer.observe(heroBench);
  }
}

// ========================================
// Event Listeners
// ========================================

// Scenario tab clicks
scenarioButtons.forEach((button) => {
  button.addEventListener("click", () => {
    const scenario = button.dataset.scenario;
    if (scenario !== currentScenario) {
      render(scenario, true);
    }
  });
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

  // Fill benchmark table
  fillTable();

  // Setup interactions
  setupCopyButtons();
  setupModal();

  // Check if reduced motion is preferred
  const prefersReducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  // Initial render
  render("warm", !prefersReducedMotion);

  // Setup scroll animations only if motion is allowed
  if (!prefersReducedMotion) {
    setupScrollAnimations();
  }
});

// Handle visibility change to re-trigger animations
document.addEventListener("visibilitychange", () => {
  if (document.visibilityState === "visible") {
    const prefersReducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    if (!prefersReducedMotion) {
      render(currentScenario, true);
    }
  }
});
