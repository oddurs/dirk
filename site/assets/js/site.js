// dirk — the two pieces of behaviour the site has.
//
// Everything else is HTML and CSS on purpose: a documentation site that needs
// a framework to show text has made a decision it will regret.

// ── The theme ───────────────────────────────────────────────────────────────
// Dark unless the system says otherwise; the toggle overrides the system in
// both directions and is remembered. base.html applies the stored choice
// before the first paint, so this only handles changing it.
(function theme() {
  const button = document.querySelector("[data-theme-toggle]");
  if (!button) return;

  const dark = () =>
    document.documentElement.dataset.theme
      ? document.documentElement.dataset.theme === "dark"
      : !window.matchMedia("(prefers-color-scheme: light)").matches;

  const label = () =>
    button.setAttribute("title", dark() ? "Switch to light" : "Switch to dark");

  label();
  button.addEventListener("click", () => {
    const next = dark() ? "light" : "dark";
    document.documentElement.dataset.theme = next;
    try {
      localStorage.setItem("dirk-theme", next);
    } catch (e) {
      // Private browsing. The choice lasts for this page rather than forever,
      // which is better than refusing to change at all.
    }
    label();
  });
})();

// ── Copying a command ───────────────────────────────────────────────────────
// The prompt is chrome and is never copied: a `$` pasted into a shell is an
// error message waiting to happen.
(function copy() {
  for (const block of document.querySelectorAll(".command")) {
    const code = block.querySelector("code");
    if (!code || block.querySelector("[data-copy]")) continue;

    const button = document.createElement("button");
    button.className = "ghost";
    button.type = "button";
    button.dataset.copy = "";
    button.textContent = "⧉";
    button.setAttribute("aria-label", "Copy this command");
    button.style.marginInlineStart = "auto";

    button.addEventListener("click", async () => {
      try {
        await navigator.clipboard.writeText(code.textContent.trim());
        button.textContent = "✓";
      } catch (e) {
        button.textContent = "✕";
      }
      setTimeout(() => (button.textContent = "⧉"), 1200);
    });

    block.append(button);
  }
})();
