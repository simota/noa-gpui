"""Functional checks for site/: anchors, mobile menu, theme gallery, copy, keyboard, reduced motion.
Usage: python3 interact.py BASE_URL OUT_DIR
"""
import glob, json, os, sys
from playwright.sync_api import sync_playwright

base, out = sys.argv[1], sys.argv[2]
exe = sorted(glob.glob(os.path.expanduser("~/Library/Caches/ms-playwright/chromium_headless_shell-*/chrome-headless-shell-mac-arm64/chrome-headless-shell")))
res = {}
with sync_playwright() as p:
    b = p.chromium.launch(executable_path=exe[-1] if exe else None)
    # Desktop
    ctx = b.new_context(viewport={"width": 1440, "height": 900})
    ctx.grant_permissions(["clipboard-read", "clipboard-write"], origin=base.rstrip("/"))
    pg = ctx.new_page()
    pg.goto(base, wait_until="networkidle")
    anchors = pg.eval_on_selector_all("a[href^='#']", "as => as.map(a => a.getAttribute('href'))")
    res["missing_anchor_targets"] = [h for h in set(anchors) if h != "#" and pg.locator(h).count() == 0]
    res["external_links"] = sorted(set(pg.eval_on_selector_all("a[href^='http']", "as => as.map(a => a.href)")))
    pg.click("text=3 ステップで動かす")
    pg.wait_for_timeout(900)
    res["hero_cta_scroll_to_start"] = pg.evaluate("Math.round(document.querySelector('#start').getBoundingClientRect().top)")
    pg.evaluate("document.querySelector('#design').scrollIntoView()")
    pg.wait_for_timeout(800)
    pg.click("[data-design='aero']"); pg.click("[data-appearance='light']")
    pg.wait_for_timeout(700)
    res["theme_after_click"] = pg.evaluate("[document.querySelector('[data-theme-image]').getAttribute('src'), document.querySelector('[data-theme-label]').textContent, document.querySelector('[data-design=aero]').getAttribute('aria-pressed')]")
    pg.locator("#design .theme-stage").screenshot(path=f"{out}/theme-aero-light.png")
    pg.click("[data-design='liquid-glass']"); pg.click("[data-appearance='dark']"); pg.wait_for_timeout(700)
    pg.locator("#design .theme-stage").screenshot(path=f"{out}/theme-liquid-dark.png")
    pg.locator("[data-copy]").first.click()
    pg.wait_for_timeout(200)
    res["copy_button_text"] = pg.locator("[data-copy]").first.text_content()
    res["clipboard"] = pg.evaluate("navigator.clipboard.readText()")
    # Keyboard: first tab stops
    pg.goto(base, wait_until="networkidle")
    stops = []
    for _ in range(10):
        pg.keyboard.press("Tab")
        stops.append(pg.evaluate("(() => { const e = document.activeElement; return (e.textContent || e.getAttribute('aria-label') || e.tagName).trim().slice(0, 24) })()"))
    res["tab_order"] = stops
    pg.keyboard.press("Shift+Tab"); 
    pg.goto(base, wait_until="networkidle"); pg.keyboard.press("Tab")
    pg.screenshot(path=f"{out}/focus-skip-link.png", clip={"x": 0, "y": 0, "width": 600, "height": 120})
    # FAQ toggle with keyboard
    pg.locator("#faq summary").first.focus(); pg.keyboard.press("Enter")
    res["faq_opens_with_enter"] = pg.evaluate("document.querySelector('#faq details').open")
    ctx.close()
    # Mobile menu
    ctx = b.new_context(viewport={"width": 375, "height": 812}, device_scale_factor=2, has_touch=True)
    pg = ctx.new_page(); pg.goto(base, wait_until="networkidle")
    res["menu_hidden_initially"] = pg.evaluate("document.querySelector('[data-nav-list]').hidden")
    pg.click("[data-nav-toggle]"); pg.wait_for_timeout(300)
    pg.screenshot(path=f"{out}/mobile-menu-open.png")
    res["menu_expanded"] = pg.get_attribute("[data-nav-toggle]", "aria-expanded")
    pg.click("[data-nav-list] a[href='#performance']"); pg.wait_for_timeout(900)
    res["menu_closed_after_link"] = pg.evaluate("document.querySelector('[data-nav-list]').hidden")
    res["performance_top_after_nav"] = pg.evaluate("Math.round(document.querySelector('#performance').getBoundingClientRect().top)")
    sizes = pg.evaluate("""() => [...document.querySelectorAll('a, button, summary')].filter(e => e.offsetParent).map(e => { const r = e.getBoundingClientRect(); return [ (e.textContent||'').trim().slice(0,20), Math.round(r.width), Math.round(r.height)] }).filter(([, w, h]) => h < 24 || w < 24)""")
    res["small_tap_targets_375"] = sizes
    ctx.close()
    # Reduced motion: content visible without scrolling animations
    ctx = b.new_context(viewport={"width": 1440, "height": 900}, reduced_motion="reduce")
    pg = ctx.new_page(); pg.goto(base, wait_until="networkidle")
    res["reduced_motion_hidden_reveals"] = pg.evaluate("[...document.querySelectorAll('.reveal')].filter(e => getComputedStyle(e).opacity !== '1').length")
    ctx.close()
    # No JS: content readable
    ctx = b.new_context(viewport={"width": 1440, "height": 900}, java_script_enabled=False)
    pg = ctx.new_page(); pg.goto(base, wait_until="networkidle")
    res["nojs_hidden_reveals"] = pg.evaluate("[...document.querySelectorAll('.reveal, [data-enter]')].filter(e => getComputedStyle(e).opacity !== '1').length")
    ctx.close()
    # main.js fails to load: content must still appear (inline fallback after 2.5 s)
    ctx = b.new_context(viewport={"width": 1440, "height": 900})
    pg = ctx.new_page(); pg.route("**/main.js", lambda r: r.abort())
    pg.goto(base, wait_until="networkidle"); pg.wait_for_timeout(3000)
    res["mainjs_blocked_hidden"] = pg.evaluate("[...document.querySelectorAll('.reveal, [data-enter]')].filter(e => getComputedStyle(e).opacity !== '1').length")
    ctx.close()
    b.close()
print(json.dumps(res, ensure_ascii=False, indent=1))
