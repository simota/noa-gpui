"""Regression check for the theme gallery: a slow decode of an earlier choice must
not overwrite a later one (PR #1 review).

Usage: python3 stale_decode.py BASE_URL   (exit 1 when the stale image wins)
"""
import glob, os, sys
from playwright.sync_api import sync_playwright

base = sys.argv[1]
# Hold every decode() until the test releases it, so the order of completion is chosen.
HOLD = """
window.__decodes = [];
HTMLImageElement.prototype.decode = function () {
  return new Promise((resolve) => window.__decodes.push({ src: this.src, resolve }));
};
"""
exe = sorted(glob.glob(os.path.expanduser("~/Library/Caches/ms-playwright/chromium_headless_shell-*/chrome-headless-shell-mac-arm64/chrome-headless-shell")))
with sync_playwright() as p:
    b = p.chromium.launch(executable_path=exe[-1] if exe else None)
    page = b.new_page(viewport={"width": 1440, "height": 900})
    page.add_init_script(HOLD)
    page.goto(base, wait_until="networkidle")
    page.click("[data-design='aero']")
    page.click("[data-design='metro']")
    # Finish the later request first, then the earlier one.
    page.evaluate("window.__decodes.find(d => d.src.includes('metro')).resolve()")
    page.wait_for_timeout(100)
    page.evaluate("window.__decodes.find(d => d.src.includes('aero')).resolve()")
    page.wait_for_timeout(100)
    shown = page.evaluate("document.querySelector('[data-theme-image]').getAttribute('src')")
    label = page.evaluate("document.querySelector('[data-theme-label]').textContent")
    swapping = page.evaluate("document.querySelector('[data-theme-frame]').classList.contains('is-swapping')")
    b.close()
print("image:", shown, "| label:", label, "| still swapping:", swapping)
sys.exit(0 if "metro" in shown and not swapping else 1)
