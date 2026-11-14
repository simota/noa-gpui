// Run with Node's standard library only:
// node --experimental-vm-modules --test apps/web/tests/loader.test.mjs
//
// Runs the module script from index.html with synthetic backend modules and a small
// DOM fixture. It does not cover a real browser, Wasm/GPU, CSS layout or screen
// readers. NOA_WEB_HTML can point to a baseline HTML.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { setImmediate } from "node:timers/promises";
import { test } from "node:test";
import vm from "node:vm";

const html = readFileSync(process.env.NOA_WEB_HTML || new URL("../index.html", import.meta.url), "utf8");
const script = html.match(/<script type="module">([\s\S]*?)<\/script>/)?.[1];
assert.ok(script, "the real loader's module script is present");

function deferred() {
  let resolve;
  const promise = new Promise(done => { resolve = done; });
  return { promise, resolve };
}

async function harness({ query = "?scene=catalog", gpu = false, backends = {}, width = 1280, height = 800, dpr = 2 } = {}) {
  const events = { imports: [], starts: [], canvases: [], info: [], warnings: [], reloads: 0, probes: 0, devicesDestroyed: 0 };
  const elements = new Map();
  const document = { documentElement: { dataset: {} }, activeElement: null };

  function element(tag, attrs = "", text = "") {
    const attributes = new Map([...attrs.matchAll(/([\w-]+)="([^"]*)"/g)].map(m => [m[1], m[2]]));
    const listeners = new Map();
    return {
      tagName: tag.toUpperCase(),
      textContent: text,
      hidden: /(?:^|\s)hidden(?:\s|=|$)/.test(attrs),
      children: [],
      clientWidth: width,
      clientHeight: height,
      setAttribute(name, value) { attributes.set(name, String(value)); },
      removeAttribute(name) { attributes.delete(name); },
      getAttribute(name) { return attributes.get(name) ?? null; },
      replaceChildren(...children) { this.children = children; },
      addEventListener(type, callback) { listeners.set(type, callback); },
      click() { listeners.get("click")?.(); },
      focus() { document.activeElement = this; },
    };
  }

  // Read the real markup's IDs, initial text and attributes. Missing/renamed
  // elements therefore fail rather than being manufactured by the fixture.
  for (const m of html.matchAll(/<([a-z][\w-]*)\b([^>]*)>([^<]*)/g)) {
    const id = m[2].match(/\bid="([^"]+)"/)?.[1];
    if (id) elements.set(id, element(m[1], m[2], m[3]));
  }
  document.getElementById = id => elements.get(id) ?? null;
  document.createElement = tag => {
    const node = element(tag);
    if (tag === "canvas") events.canvases.push(node);
    return node;
  };
  const location = { search: query, reload() { events.reloads++; } };
  const navigator = gpu ? {
    gpu: {
      async requestAdapter() {
        events.probes++;
        if (gpu === "no-adapter") return null;
        if (gpu === "adapter-error") throw new Error("adapter probe failed");
        return { async requestDevice() {
          if (gpu === "device-error") throw new Error("device probe failed");
          return { destroy() { events.devicesDestroyed++; } };
        } };
      },
    },
  } : {};
  const context = vm.createContext({
    document, navigator, location, window: { devicePixelRatio: dpr }, URLSearchParams,
    console: {
      info(...args) { events.info.push(args); },
      warn(...args) { events.warnings.push(args); },
    },
  });
  const module = new vm.SourceTextModule(script, {
    context,
    async importModuleDynamically(specifier) {
      events.imports.push(specifier);
      const behavior = backends[specifier] ?? {};
      if (behavior.importError) throw new Error(behavior.importError);
      const backend = new vm.SyntheticModule(["default", "start"], function () {
        this.setExport("default", async () => {
          if (behavior.load) await behavior.load;
          if (behavior.loadError) throw new Error(behavior.loadError);
        });
        this.setExport("start", async (canvas, passedQuery) => {
          events.starts.push({ specifier, canvas, query: passedQuery });
          if (behavior.startError) throw new Error(behavior.startError);
        });
      }, { context });
      await backend.link(() => { throw new Error("unexpected static backend import"); });
      await backend.evaluate();
      return backend;
    },
  });
  await module.link(() => { throw new Error("unexpected static loader import"); });
  const completed = module.evaluate();
  // Let the module reach an asynchronous backend boundary without a timing delay.
  await setImmediate();
  return { events, elements, document, location, completed };
}

test("without WebGPU, the WebGL2 build starts and receives the exact query and canvas size", async () => {
  const query = "?scene=minios&count=13&unrecognized=value%20with%20spaces";
  const h = await harness({ query, width: 321, height: 481, dpr: 1.5 });
  await h.completed;
  assert.deepEqual(h.events.imports, ["./webgl2.js"]);
  assert.equal(h.events.starts[0].query, query);
  assert.equal(h.events.starts[0].canvas.width, 482);
  assert.equal(h.events.starts[0].canvas.height, 722);
  assert.equal(h.document.documentElement.dataset.backend, "webgl2");
  assert.equal(h.events.warnings.length, 0);
  assert.equal(h.events.info.length, 1);
});

test("a usable WebGPU probe destroys its temporary device and prefers WebGPU", async () => {
  const h = await harness({ gpu: true });
  await h.completed;
  assert.deepEqual(h.events.imports, ["./webgpu.js"]);
  assert.equal(h.events.probes, 1);
  assert.equal(h.events.devicesDestroyed, 1);
  assert.equal(h.document.documentElement.dataset.backend, "webgpu");
});

test("missing adapters and failed adapter/device probes use WebGL2", async () => {
  for (const gpu of ["no-adapter", "adapter-error", "device-error"]) {
    const h = await harness({ gpu });
    await h.completed;
    assert.deepEqual(h.events.imports, ["./webgl2.js"], gpu);
    assert.equal(h.document.documentElement.dataset.backend, "webgl2", gpu);
  }
});

test("a failed WebGPU start retries WebGL2 on a fresh canvas and keeps diagnostics", async () => {
  const h = await harness({ gpu: true, backends: { "./webgpu.js": { startError: "first device failed" } } });
  await h.completed;
  assert.deepEqual(h.events.imports, ["./webgpu.js", "./webgl2.js"]);
  assert.equal(h.events.canvases.length, 2);
  assert.notEqual(h.events.starts[0].canvas, h.events.starts[1].canvas);
  assert.equal(h.elements.get("app").children[0], h.events.starts[1].canvas);
  assert.equal(h.document.documentElement.dataset.backend, "webgl2");
  assert.equal(h.events.warnings.length, 1);
  assert.match(String(h.events.warnings[0][1]), /first device failed/);
});

test("import and Wasm initialization errors also fall back", async () => {
  for (const first of [{ importError: "download failed" }, { loadError: "wasm initialization failed" }]) {
    const h = await harness({ gpu: true, backends: { "./webgpu.js": first } });
    await h.completed;
    assert.deepEqual(h.events.imports, ["./webgpu.js", "./webgl2.js"]);
    assert.equal(h.events.canvases.length, 1, "only a backend that starts needs a canvas");
    assert.equal(h.document.documentElement.dataset.backend, "webgl2");
  }
});

test("forced backends preserve the existing single-attempt behavior, including unknown values", async () => {
  for (const backend of ["webgpu", "webgl2", "unknown"]) {
    const specifier = `./${backend}.js`;
    const h = await harness({ query: `?scene=catalog&backend=${backend}`, gpu: true, backends: { [specifier]: { startError: "forced failure" } } });
    await h.completed;
    assert.deepEqual(h.events.imports, [specifier]);
    assert.equal(h.events.probes, 0);
    assert.equal(h.document.documentElement.dataset.backend, undefined);
    assert.match(h.elements.get("error").textContent, /forced failure/);
    assert.equal(h.events.warnings.length, 1);
  }
});

test("zero-sized hosts still create a nonzero canvas", async () => {
  const h = await harness({ width: 0, height: 0, dpr: 0 });
  await h.completed;
  assert.equal(h.events.canvases[0].width, 1);
  assert.equal(h.events.canvases[0].height, 1);
});

test("loading has a named status and stays visible until the backend starts", async () => {
  const load = deferred();
  const h = await harness({ backends: { "./webgl2.js": { load: load.promise } } });
  try {
    assert.ok(h.elements.get("startup"), "there is a startup surface");
    assert.equal(h.elements.get("startup").hidden, false);
    assert.equal(h.elements.get("app").getAttribute("aria-busy"), "true");
    assert.equal(h.elements.get("startup-status").getAttribute("role"), "status");
    assert.match(h.elements.get("startup-status").textContent, /Loading/);
  } finally {
    load.resolve();
    await h.completed;
  }
  assert.equal(h.elements.get("startup").hidden, true);
  assert.equal(h.elements.get("app").getAttribute("aria-busy"), null);
});

test("fallback remains visibly in progress until the second backend is ready", async () => {
  const load = deferred();
  const h = await harness({ gpu: true, backends: {
    "./webgpu.js": { startError: "unavailable" },
    "./webgl2.js": { load: load.promise },
  } });
  try {
    assert.ok(h.elements.get("startup-status"), "fallback has a status region");
    assert.match(h.elements.get("startup-status").textContent, /Trying a compatible option/);
    assert.equal(h.elements.get("startup").hidden, false);
    assert.equal(h.elements.get("recovery").hidden, true);
  } finally {
    load.resolve();
    await h.completed;
  }
  assert.equal(h.elements.get("startup").hidden, true);
  assert.equal(h.document.documentElement.dataset.backend, "webgl2");
});

test("complete failure exposes recovery, keeps both errors, and focuses the retry button", async () => {
  const h = await harness({ gpu: true, backends: {
    "./webgpu.js": { startError: "first failed" },
    "./webgl2.js": { startError: "second failed" },
  } });
  await h.completed;
  assert.ok(h.elements.get("recovery"), "there is an actionable recovery state");
  assert.equal(h.elements.get("startup").hidden, false);
  assert.equal(h.elements.get("startup-status").hidden, true);
  assert.equal(h.elements.get("recovery").hidden, false);
  assert.equal(h.elements.get("app").getAttribute("aria-busy"), null);
  assert.equal(h.elements.get("error-summary").getAttribute("role"), "alert");
  assert.ok(h.elements.get("error-summary").textContent.trim());
  assert.match(h.elements.get("error").textContent, /webgpu: Error: first failed/);
  assert.match(h.elements.get("error").textContent, /webgl2: Error: second failed/);
  assert.equal(h.events.warnings.length, 2);
  assert.equal(h.document.activeElement, h.elements.get("retry"));
});

test("retry reloads the whole page without replacing its scene or forced backend", async () => {
  const query = "?scene=minios&backend=webgl2&count=100";
  const h = await harness({ query, backends: { "./webgl2.js": { importError: "offline" } } });
  await h.completed;
  const retry = h.elements.get("retry");
  assert.ok(retry, "a native button provides the retry action");
  assert.equal(retry.tagName, "BUTTON");
  assert.equal(retry.getAttribute("type"), "button");
  retry.click();
  assert.equal(h.events.reloads, 1);
  assert.equal(h.location.search, query);
});
