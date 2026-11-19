//! Standard Web APIs installed into the Boa JS runtime (`EventTarget`,
//! timers, `localStorage`, `URL`, `window.history`, `fetch`, `Canvas 2D`).

pub const WEB_APIS_JS: &str = r#"
(function (global) {
    // ---- 5.1 Standards-Compliant Event & 3-Phase EventTarget ----
    class Event {
        constructor(type, init) {
            this.type = String(type);
            this.bubbles = !!(init && init.bubbles);
            this.cancelable = !!(init && init.cancelable);
            this.defaultPrevented = false;
            this.cancelBubble = false;
            this.target = null;
            this.currentTarget = null;
            this.eventPhase = 0; // 0: NONE, 1: CAPTURING, 2: AT_TARGET, 3: BUBBLING
        }
        preventDefault() {
            if (this.cancelable) this.defaultPrevented = true;
        }
        stopPropagation() {
            this.cancelBubble = true;
        }
    }
    global.Event = Event;

    const listenersMap = new WeakMap();
    const listenerRegistry = new Map(); // keyed by __nodeId for Rust-backed elements

    function getListenerStore(target) {
        if (target && typeof target.__nodeId === 'number') {
            let store = listenerRegistry.get(target.__nodeId);
            if (!store) {
                store = { capture: new Map(), bubble: new Map() };
                listenerRegistry.set(target.__nodeId, store);
            }
            return store;
        }
        let store = listenersMap.get(target);
        if (!store) {
            store = { capture: new Map(), bubble: new Map() };
            listenersMap.set(target, store);
        }
        return store;
    }

    function addEventListener(type, fn, useCapture) {
        if (typeof fn !== 'function') return;
        const store = getListenerStore(this);
        const bucket = useCapture ? store.capture : store.bubble;
        let list = bucket.get(type);
        if (!list) { list = []; bucket.set(type, list); }
        if (!list.includes(fn)) list.push(fn);
    }

    function removeEventListener(type, fn, useCapture) {
        const store = getListenerStore(this);
        const bucket = useCapture ? store.capture : store.bubble;
        const list = bucket.get(type);
        if (list) {
            const idx = list.indexOf(fn);
            if (idx !== -1) list.splice(idx, 1);
        }
    }

    function invokePhase(target, event, phase, useCapture) {
        const store = getListenerStore(target);
        const list = (useCapture ? store.capture : store.bubble).get(event.type);
        if (!list) return;
        event.eventPhase = phase;
        event.currentTarget = target;
        for (const fn of list.slice()) {
            fn.call(target, event);
        }
    }

    function dispatchEvent(event) {
        event.target = this;
        const path = [];
        let cur = this.parentNode || null;
        while (cur) {
            path.push(cur);
            cur = cur.parentNode || null;
        }
        if (this !== global) path.push(global);

        // 1. Capture phase (root -> parent)
        for (let i = path.length - 1; i >= 0; i--) {
            if (event.cancelBubble) break;
            invokePhase(path[i], event, 1, true);
        }
        // 2. Target phase
        if (!event.cancelBubble) {
            invokePhase(this, event, 2, true);
            if (!event.cancelBubble) invokePhase(this, event, 2, false);
        }
        // 3. Bubble phase (parent -> root)
        if (event.bubbles) {
            for (let i = 0; i < path.length; i++) {
                if (event.cancelBubble) break;
                invokePhase(path[i], event, 3, false);
            }
        }
        event.eventPhase = 0;
        event.currentTarget = null;
        return !event.defaultPrevented;
    }

    global.addEventListener = addEventListener.bind(global);
    global.removeEventListener = removeEventListener.bind(global);
    global.dispatchEvent = dispatchEvent.bind(global);
    if (global.document) {
        global.document.addEventListener = addEventListener.bind(global.document);
        global.document.removeEventListener = removeEventListener.bind(global.document);
        global.document.dispatchEvent = dispatchEvent.bind(global.document);
    }

    // ---- 5.2 Element helpers: classList, dataset, style, getComputedStyle, Canvas 2D ----
    function makeClassList(el) {
        const getTokens = () => (el.getAttribute('class') || '').trim().split(/\s+/).filter(Boolean);
        const setTokens = (arr) => el.setAttribute('class', arr.join(' '));
        return {
            add(...cls) {
                const s = new Set(getTokens());
                for (const c of cls) s.add(c);
                setTokens([...s]);
            },
            remove(...cls) {
                const rem = new Set(cls);
                setTokens(getTokens().filter(c => !rem.has(c)));
            },
            toggle(c, force) {
                const s = new Set(getTokens());
                const has = s.has(c);
                const next = force !== undefined ? !!force : !has;
                if (next) s.add(c); else s.delete(c);
                setTokens([...s]);
                return next;
            },
            contains(c) {
                return getTokens().includes(c);
            }
        };
    }

    function camelToKebab(s) {
        return s.replace(/[A-Z]/g, m => '-' + m.toLowerCase());
    }

    function makeStyleProxy(el) {
        return new Proxy({}, {
            get(_t, prop) {
                if (typeof prop !== 'string') return undefined;
                const raw = el.getAttribute('style') || '';
                const key = camelToKebab(prop);
                for (const part of raw.split(';')) {
                    const idx = part.indexOf(':');
                    if (idx !== -1 && part.slice(0, idx).trim().toLowerCase() === key) {
                        return part.slice(idx + 1).trim();
                    }
                }
                return '';
            },
            set(_t, prop, val) {
                if (typeof prop !== 'string') return false;
                const key = camelToKebab(prop);
                const raw = el.getAttribute('style') || '';
                const map = new Map();
                for (const part of raw.split(';')) {
                    const idx = part.indexOf(':');
                    if (idx !== -1) {
                        const k = part.slice(0, idx).trim().toLowerCase();
                        const v = part.slice(idx + 1).trim();
                        if (k) map.set(k, v);
                    }
                }
                if (val === '' || val === null || val === undefined) map.delete(key);
                else map.set(key, String(val));
                const next = [...map.entries()].map(([k, v]) => k + ': ' + v).join('; ');
                el.setAttribute('style', next);
                return true;
            }
        });
    }

    function makeDatasetProxy(el) {
        return new Proxy({}, {
            get(_t, prop) {
                if (typeof prop !== 'string') return undefined;
                return el.getAttribute('data-' + camelToKebab(prop)) || undefined;
            },
            set(_t, prop, val) {
                if (typeof prop !== 'string') return false;
                el.setAttribute('data-' + camelToKebab(prop), String(val));
                return true;
            }
        });
    }

    const canvasContexts = new Map();
    function getCanvasContext(el, kind) {
        if (kind !== '2d') return null;
        const key = el.__nodeId || el;
        let ctx = canvasContexts.get(key);
        if (ctx) return ctx;
        const ops = [];
        ctx = {
            fillStyle: '#000000',
            strokeStyle: '#000000',
            lineWidth: 1,
            ops,
            fillRect(x, y, w, h) { ops.push({ op: 'fillRect', x, y, w, h, style: this.fillStyle }); },
            strokeRect(x, y, w, h) { ops.push({ op: 'strokeRect', x, y, w, h, style: this.strokeStyle }); },
            clearRect(x, y, w, h) { ops.push({ op: 'clearRect', x, y, w, h }); },
            beginPath() { ops.push({ op: 'beginPath' }); },
            moveTo(x, y) { ops.push({ op: 'moveTo', x, y }); },
            lineTo(x, y) { ops.push({ op: 'lineTo', x, y }); },
            stroke() { ops.push({ op: 'stroke', style: this.strokeStyle, width: this.lineWidth }); },
            fill() { ops.push({ op: 'fill', style: this.fillStyle }); },
        };
        canvasContexts.set(key, ctx);
        return ctx;
    }

    global.__oxideEnhanceElement = function (el) {
        if (!el || el.__enhanced) return el;
        el.__enhanced = true;
        el.addEventListener = addEventListener.bind(el);
        el.removeEventListener = removeEventListener.bind(el);
        el.dispatchEvent = dispatchEvent.bind(el);
        el.classList = makeClassList(el);
        el.style = makeStyleProxy(el);
        el.dataset = makeDatasetProxy(el);
        if (el.tagName === 'CANVAS') {
            el.getContext = (kind) => getCanvasContext(el, kind);
        }
        return el;
    };

    global.getComputedStyle = function (el) {
        return el ? makeStyleProxy(el) : {};
    };

    // ---- 5.3 Timers & Animation Frames ----
    let nextTimerId = 1;
    const timers = new Map();
    const rafQueue = new Map();
    let nowMs = 0;

    global.setTimeout = function (fn, delay) {
        const id = nextTimerId++;
        timers.set(id, { fn, at: nowMs + Math.max(0, Number(delay) || 0), interval: null });
        return id;
    };
    global.clearTimeout = function (id) { timers.delete(id); };
    global.setInterval = function (fn, delay) {
        const id = nextTimerId++;
        const ms = Math.max(1, Number(delay) || 0);
        timers.set(id, { fn, at: nowMs + ms, interval: ms });
        return id;
    };
    global.clearInterval = function (id) { timers.delete(id); };
    global.requestAnimationFrame = function (fn) {
        const id = nextTimerId++;
        rafQueue.set(id, fn);
        return id;
    };
    global.cancelAnimationFrame = function (id) { rafQueue.delete(id); };

    global.__oxideTick = function (dtMs) {
        nowMs += Math.max(0, Number(dtMs) || 16);
        for (const [id, t] of [...timers.entries()]) {
            if (t.at <= nowMs) {
                if (t.interval !== null) {
                    t.at = nowMs + t.interval;
                } else {
                    timers.delete(id);
                }
                if (typeof t.fn === 'function') t.fn();
            }
        }
        const rafs = [...rafQueue.entries()];
        rafQueue.clear();
        for (const [, fn] of rafs) {
            if (typeof fn === 'function') fn(nowMs);
        }
    };

    // ---- 5.4 Storage, Cookie, URL, Location, History, Navigator, Fetch/XHR/WS ----
    function makeStorage() {
        const store = new Map();
        return {
            get length() { return store.size; },
            key(i) { return [...store.keys()][i] ?? null; },
            getItem(k) { return store.has(String(k)) ? store.get(String(k)) : null; },
            setItem(k, v) { store.set(String(k), String(v)); },
            removeItem(k) { store.delete(String(k)); },
            clear() { store.clear(); }
        };
    }
    global.localStorage = makeStorage();
    global.sessionStorage = makeStorage();

    const cookieMap = new Map();
    if (global.document) {
        Object.defineProperty(global.document, 'cookie', {
            get() {
                return [...cookieMap.entries()].map(([k, v]) => k + '=' + v).join('; ');
            },
            set(raw) {
                const first = String(raw).split(';')[0];
                const eq = first.indexOf('=');
                if (eq !== -1) {
                    cookieMap.set(first.slice(0, eq).trim(), first.slice(eq + 1).trim());
                }
            },
            configurable: true
        });
    }

    class URLSearchParams {
        constructor(init) {
            this._pairs = [];
            if (typeof init === 'string') {
                const s = init.startsWith('?') ? init.slice(1) : init;
                for (const part of s.split('&')) {
                    if (!part) continue;
                    const eq = part.indexOf('=');
                    const k = decodeURIComponent(eq === -1 ? part : part.slice(0, eq));
                    const v = decodeURIComponent(eq === -1 ? '' : part.slice(eq + 1));
                    this._pairs.push([k, v]);
                }
            }
        }
        get(k) { const p = this._pairs.find(([key]) => key === k); return p ? p[1] : null; }
        set(k, v) {
            const idx = this._pairs.findIndex(([key]) => key === k);
            if (idx !== -1) this._pairs[idx][1] = String(v);
            else this._pairs.push([String(k), String(v)]);
        }
        append(k, v) { this._pairs.push([String(k), String(v)]); }
        has(k) { return this._pairs.some(([key]) => key === k); }
        toString() {
            return this._pairs.map(([k, v]) => encodeURIComponent(k) + '=' + encodeURIComponent(v)).join('&');
        }
    }
    global.URLSearchParams = URLSearchParams;

    class URL {
        constructor(input) {
            const m = String(input).match(/^([a-zA-Z][a-zA-Z0-9+.-]*):\/\/([^\/?#]*)([^?#]*)(\?[^#]*)?(#.*)?$/);
            if (!m) throw new TypeError('Invalid URL: ' + input);
            this.protocol = m[1] + ':';
            this.host = m[2];
            this.hostname = m[2].split(':')[0];
            this.port = m[2].includes(':') ? m[2].split(':')[1] : '';
            this.pathname = m[3] || '/';
            this.search = m[4] || '';
            this.hash = m[5] || '';
            this.origin = this.protocol + '//' + this.host;
            this.href = this.origin + this.pathname + this.search + this.hash;
            this.searchParams = new URLSearchParams(this.search);
        }
        toString() { return this.href; }
    }
    global.URL = URL;

    global.location = {
        href: 'about:blank',
        protocol: 'about:',
        host: '',
        pathname: 'blank',
        search: '',
        hash: '',
        assign(u) { this.href = String(u); },
        replace(u) { this.href = String(u); }
    };

    const histEntries = [{ state: null, url: 'about:blank' }];
    let histIdx = 0;
    global.history = {
        get length() { return histEntries.length; },
        get state() { return histEntries[histIdx].state; },
        pushState(state, _title, url) {
            histEntries.splice(histIdx + 1);
            histEntries.push({ state: state ?? null, url: url ? String(url) : global.location.href });
            histIdx = histEntries.length - 1;
            if (url) global.location.href = String(url);
        },
        replaceState(state, _title, url) {
            histEntries[histIdx] = { state: state ?? null, url: url ? String(url) : histEntries[histIdx].url };
            if (url) global.location.href = String(url);
        },
        back() { if (histIdx > 0) histIdx--; },
        forward() { if (histIdx + 1 < histEntries.length) histIdx++; }
    };

    global.navigator = {
        userAgent: 'Mozilla/5.0 Oxide/0.2.0',
        language: 'en-US'
    };

    class Headers {
        constructor(init) {
            this._map = new Map();
            if (init) {
                for (const k of Object.keys(init)) this._map.set(k.toLowerCase(), String(init[k]));
            }
        }
        get(k) { return this._map.get(String(k).toLowerCase()) ?? null; }
        set(k, v) { this._map.set(String(k).toLowerCase(), String(v)); }
        has(k) { return this._map.has(String(k).toLowerCase()); }
    }
    class Request {
        constructor(url, init) {
            this.url = String(url);
            this.method = (init && init.method) ? String(init.method).toUpperCase() : 'GET';
            this.headers = new Headers(init && init.headers);
            this.body = init && init.body !== undefined ? init.body : null;
        }
    }
    class Response {
        constructor(body, init) {
            this._body = body !== undefined && body !== null ? String(body) : '';
            this.status = (init && init.status) || 200;
            this.ok = this.status >= 200 && this.status < 300;
            this.headers = new Headers(init && init.headers);
        }
        text() { return Promise.resolve(this._body); }
        json() { return Promise.resolve(JSON.parse(this._body)); }
    }
    global.Headers = Headers;
    global.Request = Request;
    global.Response = Response;
    global.fetch = function (input, init) {
        const req = input instanceof Request ? input : new Request(input, init);
        return Promise.resolve(new Response('', { status: 200 }));
    };

    class XMLHttpRequest {
        constructor() { this.readyState = 0; this.status = 0; this.responseText = ''; }
        open(method, url) { this._method = method; this._url = url; this.readyState = 1; }
        send() { this.readyState = 4; this.status = 200; if (typeof this.onload === 'function') this.onload(); }
    }
    global.XMLHttpRequest = XMLHttpRequest;

    class WebSocket {
        constructor(url) { this.url = String(url); this.readyState = 0; }
        send() {}
        close() { this.readyState = 3; }
    }
    global.WebSocket = WebSocket;

    if (global.window && global.window !== global) {
        Object.assign(global.window, {
            Event, addEventListener: global.addEventListener,
            removeEventListener: global.removeEventListener,
            dispatchEvent: global.dispatchEvent,
            setTimeout: global.setTimeout, clearTimeout: global.clearTimeout,
            setInterval: global.setInterval, clearInterval: global.clearInterval,
            requestAnimationFrame: global.requestAnimationFrame,
            cancelAnimationFrame: global.cancelAnimationFrame,
            localStorage: global.localStorage, sessionStorage: global.sessionStorage,
            URL: global.URL, URLSearchParams: global.URLSearchParams,
            location: global.location, history: global.history,
            navigator: global.navigator, fetch: global.fetch,
            Headers: global.Headers, Request: global.Request, Response: global.Response,
            XMLHttpRequest: global.XMLHttpRequest, WebSocket: global.WebSocket,
            getComputedStyle: global.getComputedStyle
        });
    }
})(this);
"#;
