// noa-gpui landing page: header state, mobile menu, reveal on scroll, theme gallery,
// copy buttons. No dependencies; the page reads fine with this file disabled.
(() => {
  const root = document.documentElement;
  const reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  window.noaReady = true;
  requestAnimationFrame(() => root.classList.add("is-ready"));

  // Header border once the page scrolls.
  const header = document.querySelector("[data-header]");
  const onScroll = () => header.classList.toggle("is-scrolled", window.scrollY > 8);
  requestAnimationFrame(onScroll);
  window.addEventListener("scroll", onScroll, { passive: true });

  // Mobile menu.
  const toggle = document.querySelector("[data-nav-toggle]");
  const list = document.querySelector("[data-nav-list]");
  const mobile = window.matchMedia("(max-width: 879px)");
  const setOpen = (open) => {
    toggle.setAttribute("aria-expanded", String(open));
    list.hidden = mobile.matches && !open;
  };
  setOpen(false);
  mobile.addEventListener("change", () => setOpen(false));
  toggle.addEventListener("click", () => setOpen(toggle.getAttribute("aria-expanded") !== "true"));
  list.addEventListener("click", (e) => { if (e.target.closest("a")) setOpen(false); });
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && toggle.getAttribute("aria-expanded") === "true") {
      setOpen(false);
      toggle.focus();
    }
  });

  // Reveal sections as they enter the viewport.
  const reveals = document.querySelectorAll(".reveal");
  if (reduceMotion || !("IntersectionObserver" in window)) {
    reveals.forEach((el) => el.classList.add("is-visible"));
  } else {
    const io = new IntersectionObserver((entries) => {
      for (const entry of entries) {
        if (entry.isIntersecting) {
          entry.target.classList.add("is-visible");
          io.unobserve(entry.target);
        }
      }
    }, { rootMargin: "0px 0px -8% 0px", threshold: 0.08 });
    reveals.forEach((el) => io.observe(el));
  }

  // Theme gallery: every image is a real Headless render of the same widget tree.
  const names = {
    "big-sur": "Big Sur", "liquid-glass": "Liquid Glass", "flat-vibrancy": "Flat + Vibrancy",
    aero: "Aero", metro: "Metro", "fluent-2": "Fluent 2", noa: "Noa",
  };
  const appearanceNames = { light: "ライト", dark: "ダーク" };
  const image = document.querySelector("[data-theme-image]");
  const label = document.querySelector("[data-theme-label]");
  const frame = image.parentElement;
  const designButtons = document.querySelectorAll("[data-design]");
  const appearanceButtons = document.querySelectorAll("[data-appearance]");
  const state = { design: "noa", appearance: "dark" };
  const preloaded = new Set();
  const src = (d, a) => `assets/themes/${d}-${a}.webp`;
  const preload = (url) => {
    if (preloaded.has(url)) return;
    preloaded.add(url);
    new Image().src = url;
  };

  const render = () => {
    const url = src(state.design, state.appearance);
    const text = `${names[state.design]} · ${appearanceNames[state.appearance]}`;
    designButtons.forEach((b) => b.setAttribute("aria-pressed", String(b.dataset.design === state.design)));
    appearanceButtons.forEach((b) => b.setAttribute("aria-pressed", String(b.dataset.appearance === state.appearance)));
    label.textContent = text;
    frame.dataset.appearanceNow = state.appearance;
    const swap = () => {
      image.src = url;
      image.alt = `${text} 表示の設定パネル`;
    };
    if (reduceMotion) return swap();
    frame.classList.add("is-swapping");
    const next = new Image();
    next.src = url;
    (next.decode ? next.decode() : Promise.resolve()).catch(() => {}).then(() => {
      setTimeout(() => {
        swap();
        frame.classList.remove("is-swapping");
      }, 120);
    });
  };

  designButtons.forEach((b) => {
    b.addEventListener("click", () => { state.design = b.dataset.design; render(); });
    b.addEventListener("pointerenter", () => preload(src(b.dataset.design, state.appearance)), { passive: true });
  });
  appearanceButtons.forEach((b) => {
    b.addEventListener("click", () => { state.appearance = b.dataset.appearance; render(); });
  });

  // Copy buttons.
  const status = document.querySelector("[data-copy-status]");
  document.querySelectorAll("[data-copy]").forEach((button) => {
    const original = button.textContent;
    button.addEventListener("click", async () => {
      const block = button.closest(".code-block");
      const code = block.querySelector("code").innerText.trim();
      try {
        await navigator.clipboard.writeText(code);
        button.textContent = "コピーしました";
        button.classList.add("is-done");
        status.textContent = "クリップボードにコピーしました";
      } catch {
        button.textContent = "選択してコピー";
        const range = document.createRange();
        range.selectNodeContents(block.querySelector("code"));
        const sel = window.getSelection();
        sel.removeAllRanges();
        sel.addRange(range);
      }
      setTimeout(() => {
        button.textContent = original;
        button.classList.remove("is-done");
      }, 1800);
    });
  });
})();
