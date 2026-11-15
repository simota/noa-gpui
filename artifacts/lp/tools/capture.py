"""Screenshots and layout checks for site/ at the standard widths.

Usage: python3 capture.py BASE_URL OUT_DIR
Writes OUT_DIR/{w}-hero.png and OUT_DIR/{w}-full.png and prints a JSON report of
horizontal overflow, elements wider than the viewport, broken images, console errors
and failed requests per width.
"""
import json, sys
from playwright.sync_api import sync_playwright

base, out = sys.argv[1], sys.argv[2]
WIDTHS = [(320, 640), (375, 812), (768, 1024), (1024, 768), (1440, 900), (1920, 1080)]

CHECK = """() => {
  const vw = document.documentElement.clientWidth;
  const wide = [];
  for (const el of document.querySelectorAll('body *')) {
    const r = el.getBoundingClientRect();
    if (r.width === 0) continue;
    if (el.closest('pre, .hero-visual, .showcase, .code-block')) continue;
    if (r.right > vw + 1 || r.left < -1) wide.push((el.className || el.tagName) + ' ' + Math.round(r.left) + '..' + Math.round(r.right));
  }
  const broken = [...document.images].filter(i => i.complete && i.naturalWidth === 0).map(i => i.src);
  return { scrollWidth: document.documentElement.scrollWidth, vw, overflowX: document.documentElement.scrollWidth > vw, wide: wide.slice(0, 12), broken };
}"""

report = {}
with sync_playwright() as p:
    import glob, os
    exe = sorted(glob.glob(os.path.expanduser("~/Library/Caches/ms-playwright/chromium_headless_shell-*/chrome-headless-shell-mac-arm64/chrome-headless-shell")))
    browser = p.chromium.launch(executable_path=exe[-1] if exe else None)
    for w, h in WIDTHS:
        ctx = browser.new_context(viewport={"width": w, "height": h}, device_scale_factor=2 if w <= 768 else 1)
        page = ctx.new_page()
        errors, failed = [], []
        page.on("console", lambda m: errors.append(m.text) if m.type == "error" else None)
        page.on("pageerror", lambda e: errors.append(str(e)))
        page.on("requestfailed", lambda r: failed.append(r.url))
        page.goto(base, wait_until="networkidle")
        page.wait_for_timeout(1300)
        page.screenshot(path=f"{out}/{w}-hero.png")
        # Scroll through so reveal-on-scroll content is in its final state.
        height = page.evaluate("document.body.scrollHeight")
        for y in range(0, height, h // 2):
            page.evaluate(f"window.scrollTo(0, {y})")
            page.wait_for_timeout(60)
        page.wait_for_timeout(900)
        page.evaluate("window.scrollTo(0, 0)")
        page.wait_for_timeout(300)
        page.screenshot(path=f"{out}/{w}-full.png", full_page=True)
        r = page.evaluate(CHECK)
        r["errors"], r["failed"] = errors, failed
        report[w] = r
        ctx.close()
    browser.close()
print(json.dumps(report, ensure_ascii=False, indent=1))
